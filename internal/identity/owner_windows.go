//go:build windows

package identity

import (
	"errors"
	"io/fs"
	"os"
	"strings"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"
)

func ownerChecksAvailable() bool { return true }

func secureMode(os.FileInfo, bool) bool { return true }

var replaceFileW = windows.NewLazySystemDLL("kernel32.dll").NewProc("ReplaceFileW")

func replaceFile(oldPath, newPath string) error {
	oldPointer, err := windows.UTF16PtrFromString(oldPath)
	if err != nil {
		return err
	}
	newPointer, err := windows.UTF16PtrFromString(newPath)
	if err != nil {
		return err
	}
	result, _, callErr := replaceFileW.Call(
		uintptr(unsafe.Pointer(newPointer)),
		uintptr(unsafe.Pointer(oldPointer)),
		0,
		0,
		0,
		0,
	)
	if result != 0 {
		return nil
	}
	if errors.Is(callErr, syscall.ERROR_FILE_NOT_FOUND) {
		return windows.Rename(oldPath, newPath)
	}
	return callErr
}

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
	if err := securePath(path, false); err != nil {
		_ = file.Close()
		return nil, err
	}
	info, err := file.Stat()
	pathInfo, pathErr := os.Lstat(path)
	if err != nil || pathErr != nil || pathInfo.Mode()&os.ModeSymlink != 0 || !os.SameFile(info, pathInfo) ||
		!info.Mode().IsRegular() || !securePathOwned(path, info) {
		_ = file.Close()
		return nil, ErrUnsafeStorage
	}
	overlapped := new(windows.Overlapped)
	if err := windows.LockFileEx(windows.Handle(file.Fd()), windows.LOCKFILE_EXCLUSIVE_LOCK, 0, 1, 0, overlapped); err != nil {
		_ = file.Close()
		return nil, err
	}
	return func() {
		_ = errors.Join(windows.UnlockFileEx(windows.Handle(file.Fd()), 0, 1, 0, overlapped), file.Close())
	}, nil
}

func securePath(path string, directory bool) error {
	descriptor, err := ownerDescriptor(directory)
	if err != nil {
		return err
	}
	dacl, _, err := descriptor.DACL()
	if err != nil {
		return err
	}
	return windows.SetNamedSecurityInfo(path, windows.SE_FILE_OBJECT,
		windows.DACL_SECURITY_INFORMATION|windows.PROTECTED_DACL_SECURITY_INFORMATION,
		nil, nil, dacl, nil)
}

func securePathOwned(path string, info os.FileInfo) bool {
	want, err := ownerDescriptor(info.IsDir())
	if err != nil {
		return false
	}
	actual, err := windows.GetNamedSecurityInfo(path, windows.SE_FILE_OBJECT,
		windows.OWNER_SECURITY_INFORMATION|windows.DACL_SECURITY_INFORMATION)
	if err != nil {
		return false
	}
	user, err := windows.GetCurrentProcessToken().GetTokenUser()
	if err != nil {
		return false
	}
	owner, _, err := actual.Owner()
	return err == nil && owner.Equals(user.User.Sid) && descriptorDACL(actual) == descriptorDACL(want)
}

func pathOwnerMatches(path string, _ os.FileInfo) bool {
	descriptor, err := windows.GetNamedSecurityInfo(path, windows.SE_FILE_OBJECT, windows.OWNER_SECURITY_INFORMATION)
	if err != nil {
		return false
	}
	user, err := windows.GetCurrentProcessToken().GetTokenUser()
	if err != nil {
		return false
	}
	owner, _, err := descriptor.Owner()
	return err == nil && owner.Equals(user.User.Sid)
}

func ownerDescriptor(directory bool) (*windows.SECURITY_DESCRIPTOR, error) {
	user, err := windows.GetCurrentProcessToken().GetTokenUser()
	if err != nil {
		return nil, err
	}
	flags := ""
	if directory {
		flags = "OICI"
	}
	return windows.SecurityDescriptorFromString("D:P(A;" + flags + ";FA;;;" + user.User.Sid.String() + ")")
}

func descriptorDACL(descriptor *windows.SECURITY_DESCRIPTOR) string {
	value := descriptor.String()
	index := strings.Index(value, "D:")
	if index < 0 {
		return ""
	}
	value = value[index:]
	if index = strings.Index(value, "S:"); index >= 0 {
		value = value[:index]
	}
	return value
}
