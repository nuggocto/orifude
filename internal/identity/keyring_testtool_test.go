//go:build testtool

package identity

import (
	"errors"
	"testing"
)

func TestTesttoolStoreUsesOnlyApprovedFileFallback(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	store, err := NewStore()
	if err != nil {
		t.Fatal(err)
	}
	device := testDevice(t)
	if _, err := store.SavePending("River Stone", device, false); !errors.Is(err, ErrFallbackApproval) {
		t.Fatalf("unapproved fallback error = %v", err)
	}
	profile, err := store.SavePending("River Stone", device, true)
	if err != nil || !profile.Fallback {
		t.Fatalf("approved fallback = %+v, %v", profile, err)
	}
}
