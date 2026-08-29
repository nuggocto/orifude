//go:build !windows

package identity

import (
	"errors"
	"io/fs"
	"os"
	"syscall"
)

func ownerChecksAvailable() bool { return true }

func securePath(string, bool) error { return nil }

func replaceFile(oldPath, newPath string) error { return os.Rename(oldPath, newPath) }

func lockFile(path string) (func(), error) {
	if info, err := os.Lstat(path); err == nil {
		if info.Mode()&os.ModeSymlink != 0 || !info.Mode().IsRegular() || !pathOwnerMatches(path, info) {
			return nil, ErrUnsafeStorage
		}
	} else if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return nil, err
	}
	file, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0o600)
	if err != nil {
		return nil, err
	}
	if err := file.Chmod(0o600); err != nil {
		_ = file.Close()
		return nil, err
	}
	info, err := file.Stat()
	pathInfo, pathErr := os.Lstat(path)
	if err != nil || pathErr != nil || pathInfo.Mode()&os.ModeSymlink != 0 || !os.SameFile(info, pathInfo) ||
		!info.Mode().IsRegular() || !secureMode(info, false) || !securePathOwned(path, info) {
		_ = file.Close()
		return nil, ErrUnsafeStorage
	}
	if err := syscall.Flock(int(file.Fd()), syscall.LOCK_EX); err != nil {
		_ = file.Close()
		return nil, err
	}
	return func() {
		_ = errors.Join(syscall.Flock(int(file.Fd()), syscall.LOCK_UN), file.Close())
	}, nil
}

func secureMode(info os.FileInfo, _ bool) bool { return info.Mode().Perm()&0o077 == 0 }

func securePathOwned(_ string, info os.FileInfo) bool {
	stat, ok := info.Sys().(*syscall.Stat_t)
	return ok && stat.Uid == uint32(os.Getuid())
}

func pathOwnerMatches(path string, info os.FileInfo) bool { return securePathOwned(path, info) }
