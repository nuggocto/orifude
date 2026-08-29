//go:build testtool

package identity

import "errors"

var errTestCredentialStoreUnavailable = errors.New("identity: test credential store unavailable")

type unavailableKeyring struct{}

func (unavailableKeyring) Set(string, string, string) error {
	return errTestCredentialStoreUnavailable
}

func (unavailableKeyring) Get(string, string) (string, error) {
	return "", errTestCredentialStoreUnavailable
}

func (unavailableKeyring) Delete(string, string) error {
	return errTestCredentialStoreUnavailable
}

func defaultKeyring() keyringBackend { return unavailableKeyring{} }
