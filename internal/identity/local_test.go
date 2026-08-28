package identity

import (
	"crypto/rand"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/nuggocto/orifude/internal/auth"
	"github.com/zalando/go-keyring"
)

type memoryKeyring struct {
	secret string
	setErr error
}

func (m *memoryKeyring) Set(_, _, password string) error {
	if m.setErr != nil {
		return m.setErr
	}
	m.secret = password
	return nil
}
func (m *memoryKeyring) Get(_, _ string) (string, error) {
	if m.secret == "" {
		return "", keyring.ErrNotFound
	}
	return m.secret, nil
}
func (m *memoryKeyring) Delete(_, _ string) error {
	if m.secret == "" {
		return keyring.ErrNotFound
	}
	m.secret = ""
	return nil
}

func TestStoreKeepsKeyInKeyringAndOnlySafeMetadataOnDisk(t *testing.T) {
	backend := &memoryKeyring{}
	store := newStore(t.TempDir(), backend)
	device := testDevice(t)
	profile, err := store.SavePending("Maple Finch", device, false)
	if err != nil || profile.Fallback || profile.Active {
		t.Fatalf("SavePending profile = %+v, %v", profile, err)
	}
	data, err := os.ReadFile(store.identityPath())
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(data), "private_key") || strings.Contains(string(data), backend.secret) {
		t.Fatal("identity metadata contains keyring private key material")
	}
	loadedProfile, loaded, err := store.Load()
	if err != nil || loaded.Thumbprint() != device.Thumbprint() || loadedProfile.Alias != "Maple Finch" {
		t.Fatalf("Load = %+v, %v", loadedProfile, err)
	}
	active, err := store.Activate("Maple Finch")
	if err != nil || !active.Active {
		t.Fatalf("Activate = %+v, %v", active, err)
	}
	settings := Settings{Theme: "mono", ReducedMotion: true, ASCIIFallback: true, Accessible: true}
	if err := store.SaveSettings(settings); err != nil {
		t.Fatal(err)
	}
	gotSettings, err := store.LoadSettings()
	if err != nil || gotSettings != settings {
		t.Fatalf("LoadSettings = %+v, %v", gotSettings, err)
	}
	if err := store.Delete(); err != nil {
		t.Fatal(err)
	}
	if _, _, err := store.Load(); !errors.Is(err, ErrNotFound) {
		t.Fatalf("Load after Delete error = %v", err)
	}
	if _, err := os.Stat(store.settingsPath()); err != nil {
		t.Fatalf("Delete removed display settings: %v", err)
	}
}

func TestStoreRequiresApprovalForOwnerOnlyFallback(t *testing.T) {
	backend := &memoryKeyring{setErr: errors.New("credential store unavailable")}
	store := newStore(t.TempDir(), backend)
	device := testDevice(t)
	if _, err := store.SavePending("River Stone", device, false); !errors.Is(err, ErrFallbackApproval) {
		t.Fatalf("unapproved fallback error = %v", err)
	}
	if _, err := os.Stat(store.identityPath()); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("unapproved fallback wrote identity file: %v", err)
	}
	profile, err := store.SavePending("River Stone", device, true)
	if err != nil || !profile.Fallback {
		t.Fatalf("approved fallback = %+v, %v", profile, err)
	}
	info, err := os.Stat(store.identityPath())
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS != "windows" && info.Mode().Perm() != 0o600 {
		t.Fatalf("fallback mode = %o", info.Mode().Perm())
	}
	loadedProfile, loaded, err := store.Load()
	if err != nil || !loadedProfile.Fallback || loaded.Thumbprint() != device.Thumbprint() {
		t.Fatalf("Load fallback = %+v, %v", loadedProfile, err)
	}
}

func TestStoreDoesNotTreatMissingKeyringKeyAsFirstRun(t *testing.T) {
	backend := &memoryKeyring{}
	store := newStore(t.TempDir(), backend)
	if _, err := store.SavePending("River Stone", testDevice(t), false); err != nil {
		t.Fatal(err)
	}
	backend.secret = ""
	if _, _, err := store.Load(); !errors.Is(err, ErrInvalidRecord) {
		t.Fatalf("missing keyring key error = %v, want invalid record", err)
	}
}

func TestStoreRejectsSymlinkAndBroadPermissions(t *testing.T) {
	directory := t.TempDir()
	store := newStore(directory, &memoryKeyring{})
	target := filepath.Join(directory, "target")
	if err := os.WriteFile(target, []byte(`{}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, store.identityPath()); err != nil {
		t.Fatal(err)
	}
	if _, _, err := store.Load(); !errors.Is(err, ErrUnsafeStorage) {
		t.Fatalf("symlink load error = %v", err)
	}
	if err := os.Remove(store.identityPath()); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(store.identityPath(), []byte(`{}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, _, err := store.Load(); !errors.Is(err, ErrUnsafeStorage) {
		t.Fatalf("broad-permission load error = %v", err)
	}
}

func testDevice(t *testing.T) *auth.DeviceKey {
	t.Helper()
	device, err := auth.GenerateDeviceKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return device
}
