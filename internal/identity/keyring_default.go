//go:build !testtool

package identity

func defaultKeyring() keyringBackend { return systemKeyring{} }
