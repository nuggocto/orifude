//go:build windows

package identity

import (
	"os"
	"strings"

	"golang.org/x/sys/windows"
)

func ownerChecksAvailable() bool { return true }

func secureMode(os.FileInfo, bool) bool { return true }

func replaceFile(oldPath, newPath string) error { return windows.Rename(oldPath, newPath) }

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
