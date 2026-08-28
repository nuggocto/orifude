package postoffice

import (
	"testing"
	"time"

	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database"
	"github.com/nuggocto/orifude/internal/envelope"
)

func TestNewRequiresRateRetentionToCoverEnabledWindows(t *testing.T) {
	tests := []struct {
		name      string
		configure func(*Config)
		required  time.Duration
	}{
		{
			name: "hourly",
			configure: func(config *Config) {
				config.ClaimCooldown = 0
				config.ClaimPerDay = 0
				config.ReportPerDay = 0
			},
			required: time.Hour,
		},
		{
			name: "daily",
			configure: func(config *Config) {
				config.ClaimCooldown = 0
			},
			required: 24 * time.Hour,
		},
		{
			name: "cooldown",
			configure: func(config *Config) {
				config.SendPerHour = 0
				config.ClaimCooldown = 25 * time.Hour
				config.ClaimPerHour = 0
				config.ClaimPerDay = 0
				config.ReportPerDay = 0
			},
			required: 25 * time.Hour,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			config := DefaultConfig()
			test.configure(&config)
			config.RateRetention = test.required - time.Second
			if _, err := New(new(database.DB), new(auth.Verifier), new(envelope.Cipher), config); err != ErrInvalid {
				t.Fatalf("short retention error = %v, want invalid", err)
			}
			config.RateRetention = test.required
			if _, err := New(new(database.DB), new(auth.Verifier), new(envelope.Cipher), config); err != nil {
				t.Fatalf("exact retention: %v", err)
			}
		})
	}
}
