//go:build !windows

package identity

import (
	"os"
	"syscall"
)

func ownerChecksAvailable() bool { return true }

func securePath(string, bool) error { return nil }

func replaceFile(oldPath, newPath string) error { return os.Rename(oldPath, newPath) }

func secureMode(info os.FileInfo, _ bool) bool { return info.Mode().Perm()&0o077 == 0 }

func securePathOwned(_ string, info os.FileInfo) bool {
	stat, ok := info.Sys().(*syscall.Stat_t)
	return ok && stat.Uid == uint32(os.Getuid())
}
