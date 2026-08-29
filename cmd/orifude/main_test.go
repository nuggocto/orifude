package main

import "testing"

func TestResolvedVersionKeepsReleaseVersion(t *testing.T) {
	t.Parallel()

	const release = "v0.2.0"
	if got := resolvedVersion(release); got != release {
		t.Fatalf("resolvedVersion(%q) = %q", release, got)
	}
}
