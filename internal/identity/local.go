package identity

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"

	"github.com/nuggocto/orifude/internal/auth"
	"github.com/zalando/go-keyring"
)

const (
	recordVersion  = 1
	keyringService = "orifude"
	keyringAccount = "device-key"
)

var (
	ErrNotFound         = errors.New("identity: local identity not found")
	ErrFallbackApproval = errors.New("identity: owner-only file fallback requires approval")
	ErrUnsafeStorage    = errors.New("identity: local identity storage is unsafe")
	ErrInvalidRecord    = errors.New("identity: invalid local identity")
	ErrAlreadyExists    = errors.New("identity: local identity already exists")
)

type keyringBackend interface {
	Set(service, user, password string) error
	Get(service, user string) (string, error)
	Delete(service, user string) error
}

type systemKeyring struct{}

func (systemKeyring) Set(service, user, password string) error {
	return keyring.Set(service, user, password)
}
func (systemKeyring) Get(service, user string) (string, error) { return keyring.Get(service, user) }
func (systemKeyring) Delete(service, user string) error        { return keyring.Delete(service, user) }

// Profile is display-safe local identity metadata.
type Profile struct {
	Alias      string
	Thumbprint string
	Active     bool
	Fallback   bool
}

type identityRecord struct {
	Version    int    `json:"version"`
	Alias      string `json:"alias"`
	Thumbprint string `json:"thumbprint"`
	Status     string `json:"status"`
	KeyStorage string `json:"key_storage"`
	PrivateKey string `json:"private_key,omitempty"`
}

// Settings contains non-secret TUI preferences.
type Settings struct {
	Theme         string `json:"theme"`
	ReducedMotion bool   `json:"reduced_motion"`
	ASCIIFallback bool   `json:"ascii_fallback"`
	Accessible    bool   `json:"accessible"`
}

// Store owns local identity and settings persistence.
type Store struct {
	directory string
	keyring   keyringBackend
}

// NewStore uses the operating-system configuration directory and credential store.
func NewStore() (*Store, error) {
	directory, err := os.UserConfigDir()
	if err != nil {
		return nil, fmt.Errorf("find user configuration directory: %w", err)
	}
	return newStore(filepath.Join(directory, "orifude"), systemKeyring{}), nil
}

func newStore(directory string, backend keyringBackend) *Store {
	return &Store{directory: directory, keyring: backend}
}

// SavePending stores a newly generated key before registration. It returns
// ErrFallbackApproval without writing a file when the credential store fails
// and fallback has not been approved.
func (s *Store) SavePending(alias string, device *auth.DeviceKey, allowFallback bool) (Profile, error) {
	if s == nil || s.keyring == nil || alias == "" || device == nil {
		return Profile{}, ErrInvalidRecord
	}
	unlock, err := s.lockIdentity()
	if err != nil {
		return Profile{}, err
	}
	defer unlock()
	if _, err := s.readRecord(); err == nil {
		return Profile{}, ErrAlreadyExists
	} else if !errors.Is(err, ErrNotFound) {
		return Profile{}, err
	}
	_, keyringErr := s.keyring.Get(keyringService, keyringAccount)
	if keyringErr == nil {
		return Profile{}, ErrAlreadyExists
	}
	encoded, err := device.MarshalPKCS8()
	if err != nil {
		return Profile{}, err
	}
	defer clear(encoded)
	thumbprint := auth.EncodeHash(device.Thumbprint())
	record := identityRecord{Version: recordVersion, Alias: alias, Thumbprint: thumbprint, Status: "pending", KeyStorage: "keyring"}
	secret := base64.RawStdEncoding.EncodeToString(encoded)
	if errors.Is(keyringErr, keyring.ErrNotFound) {
		keyringErr = s.keyring.Set(keyringService, keyringAccount, secret)
	}
	if keyringErr == nil {
		if err := s.writeRecord(record); err != nil {
			_ = s.keyring.Delete(keyringService, keyringAccount)
			return Profile{}, err
		}
		return profile(record), nil
	}
	if !allowFallback {
		return Profile{}, ErrFallbackApproval
	}
	if !ownerChecksAvailable() {
		return Profile{}, ErrUnsafeStorage
	}
	record.KeyStorage = "file"
	record.PrivateKey = secret
	if err := s.writeRecord(record); err != nil {
		return Profile{}, err
	}
	return profile(record), nil
}

// Activate records that registration or session recovery confirmed the identity.
func (s *Store) Activate(alias string) (Profile, error) {
	unlock, err := s.lockIdentity()
	if err != nil {
		return Profile{}, err
	}
	defer unlock()
	record, err := s.readRecord()
	if err != nil {
		return Profile{}, err
	}
	if alias == "" {
		return Profile{}, ErrInvalidRecord
	}
	record.Alias = alias
	record.Status = "active"
	if err := s.writeRecord(record); err != nil {
		return Profile{}, err
	}
	return profile(record), nil
}

// Load returns validated identity metadata and its P-256 private key.
func (s *Store) Load() (Profile, *auth.DeviceKey, error) {
	unlock, err := s.lockIdentity()
	if err != nil {
		return Profile{}, nil, err
	}
	defer unlock()
	record, err := s.readRecord()
	if errors.Is(err, ErrNotFound) {
		if _, keyErr := s.keyring.Get(keyringService, keyringAccount); keyErr == nil {
			return Profile{}, nil, ErrInvalidRecord
		}
	}
	if err != nil {
		return Profile{}, nil, err
	}
	var encoded string
	switch record.KeyStorage {
	case "keyring":
		encoded, err = s.keyring.Get(keyringService, keyringAccount)
		if errors.Is(err, keyring.ErrNotFound) {
			return Profile{}, nil, ErrInvalidRecord
		}
		if err != nil {
			return Profile{}, nil, fmt.Errorf("load device key: %w", err)
		}
	case "file":
		encoded = record.PrivateKey
	default:
		return Profile{}, nil, ErrInvalidRecord
	}
	keyData, err := base64.RawStdEncoding.DecodeString(encoded)
	if err != nil || base64.RawStdEncoding.EncodeToString(keyData) != encoded {
		return Profile{}, nil, ErrInvalidRecord
	}
	defer clear(keyData)
	device, err := auth.ParseDeviceKey(keyData)
	if err != nil || auth.EncodeHash(device.Thumbprint()) != record.Thumbprint {
		return Profile{}, nil, ErrInvalidRecord
	}
	return profile(record), device, nil
}

// Delete removes local identity material without changing display settings.
func (s *Store) Delete() error {
	unlock, err := s.lockIdentity()
	if err != nil {
		return err
	}
	defer unlock()
	record, err := s.readRecord()
	if errors.Is(err, ErrNotFound) {
		if _, keyErr := s.keyring.Get(keyringService, keyringAccount); keyErr != nil {
			return nil
		}
		if err := s.keyring.Delete(keyringService, keyringAccount); err != nil && !errors.Is(err, keyring.ErrNotFound) {
			return fmt.Errorf("delete device key: %w", err)
		}
		return nil
	}
	if err != nil {
		return err
	}
	if record.KeyStorage == "keyring" {
		if err := s.keyring.Delete(keyringService, keyringAccount); err != nil && !errors.Is(err, keyring.ErrNotFound) {
			return fmt.Errorf("delete device key: %w", err)
		}
	}
	if err := os.Remove(s.identityPath()); err != nil && !errors.Is(err, fs.ErrNotExist) {
		return fmt.Errorf("delete local identity: %w", err)
	}
	return nil
}

// LoadSettings returns defaults when no settings file exists.
func (s *Store) LoadSettings() (Settings, error) {
	data, err := s.readFile(s.settingsPath())
	if errors.Is(err, ErrNotFound) {
		return Settings{Theme: "auto"}, nil
	}
	if err != nil {
		return Settings{}, err
	}
	var settings Settings
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&settings); err != nil || settings.Theme != "auto" && settings.Theme != "light" && settings.Theme != "dark" && settings.Theme != "mono" {
		return Settings{}, ErrInvalidRecord
	}
	if err := decoder.Decode(&struct{}{}); err != io.EOF {
		return Settings{}, ErrInvalidRecord
	}
	return settings, nil
}

// SaveSettings atomically stores non-secret preferences.
func (s *Store) SaveSettings(settings Settings) error {
	if settings.Theme != "auto" && settings.Theme != "light" && settings.Theme != "dark" && settings.Theme != "mono" {
		return ErrInvalidRecord
	}
	data, err := json.Marshal(settings)
	if err != nil {
		return err
	}
	return s.writeFile(s.settingsPath(), data)
}

func (s *Store) readRecord() (identityRecord, error) {
	data, err := s.readFile(s.identityPath())
	if err != nil {
		return identityRecord{}, err
	}
	var record identityRecord
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&record); err != nil || record.Version != recordVersion || record.Alias == "" || record.Thumbprint == "" ||
		record.Status != "pending" && record.Status != "active" || record.KeyStorage != "keyring" && record.KeyStorage != "file" ||
		record.KeyStorage == "keyring" && record.PrivateKey != "" || record.KeyStorage == "file" && record.PrivateKey == "" {
		return identityRecord{}, ErrInvalidRecord
	}
	if err := decoder.Decode(&struct{}{}); err != io.EOF {
		return identityRecord{}, ErrInvalidRecord
	}
	if record.KeyStorage == "file" && !ownerChecksAvailable() {
		return identityRecord{}, ErrUnsafeStorage
	}
	return record, nil
}

func (s *Store) writeRecord(record identityRecord) error {
	data, err := json.Marshal(record)
	if err != nil {
		return err
	}
	return s.writeFile(s.identityPath(), data)
}

func profile(record identityRecord) Profile {
	return Profile{Alias: record.Alias, Thumbprint: record.Thumbprint, Active: record.Status == "active", Fallback: record.KeyStorage == "file"}
}

func (s *Store) identityPath() string { return filepath.Join(s.directory, "identity.json") }
func (s *Store) settingsPath() string { return filepath.Join(s.directory, "settings.json") }
func (s *Store) lockPath() string     { return filepath.Join(s.directory, "identity.lock") }

func (s *Store) lockIdentity() (func(), error) {
	if s == nil || s.directory == "" {
		return nil, ErrInvalidRecord
	}
	if err := ensureDirectory(s.directory); err != nil {
		return nil, err
	}
	return lockFile(s.lockPath())
}

func (s *Store) readFile(path string) ([]byte, error) {
	if s == nil || s.directory == "" {
		return nil, ErrInvalidRecord
	}
	if err := validateDirectory(s.directory); err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return nil, ErrNotFound
		}
		return nil, err
	}
	info, err := os.Lstat(path)
	if errors.Is(err, fs.ErrNotExist) {
		return nil, ErrNotFound
	}
	if err != nil {
		return nil, err
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.Mode().IsRegular() || !secureMode(info, false) || !securePathOwned(path, info) {
		return nil, ErrUnsafeStorage
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	openedInfo, err := file.Stat()
	if err != nil || !openedInfo.Mode().IsRegular() || !secureMode(openedInfo, false) || !securePathOwned(path, openedInfo) {
		return nil, ErrUnsafeStorage
	}
	data, err := io.ReadAll(io.LimitReader(file, (64<<10)+1))
	if err != nil {
		return nil, err
	}
	if len(data) > 64<<10 {
		return nil, ErrInvalidRecord
	}
	return data, nil
}

func (s *Store) writeFile(path string, data []byte) error {
	if len(data) > 64<<10 {
		return ErrInvalidRecord
	}
	if err := ensureDirectory(s.directory); err != nil {
		return err
	}
	temporary, err := os.CreateTemp(s.directory, ".orifude-*")
	if err != nil {
		return err
	}
	temporaryPath := temporary.Name()
	keep := false
	defer func() {
		_ = temporary.Close()
		if !keep {
			_ = os.Remove(temporaryPath)
		}
	}()
	if err := temporary.Chmod(0o600); err != nil {
		return err
	}
	if err := securePath(temporaryPath, false); err != nil {
		return err
	}
	if _, err := temporary.Write(data); err != nil {
		return err
	}
	if err := temporary.Sync(); err != nil {
		return err
	}
	if err := temporary.Close(); err != nil {
		return err
	}
	if err := replaceFile(temporaryPath, path); err != nil {
		return err
	}
	keep = true
	return nil
}

func ensureDirectory(directory string) error {
	if directory == "" {
		return ErrInvalidRecord
	}
	if err := os.MkdirAll(directory, 0o700); err != nil {
		return err
	}
	info, err := os.Lstat(directory)
	if err != nil {
		return err
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() || !pathOwnerMatches(directory, info) {
		return ErrUnsafeStorage
	}
	if err := os.Chmod(directory, 0o700); err != nil {
		return err
	}
	if err := securePath(directory, true); err != nil {
		return err
	}
	return validateDirectory(directory)
}

func validateDirectory(directory string) error {
	info, err := os.Lstat(directory)
	if err != nil {
		return err
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() || !secureMode(info, true) || !securePathOwned(directory, info) {
		return ErrUnsafeStorage
	}
	return nil
}
