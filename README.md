# Orifude

> Send a letter into the quiet and let a stranger find it.

Orifude is a pseudonymous, one-to-one letter exchange for the terminal. Choose a
private alias, write a short letter, and leave it for one unrelated recipient.
They may send one reply, and the exchange becomes a keepsake for both people.

Aliases are visible only to matched strangers and cannot be searched. There is
no public feed, profile, follower count, recipient picker, or endless chat. The
terminal interface is the application. [orifude.com](https://orifude.com) will
explain the project and distribute releases without access to letters.

Orifude is not end-to-end encrypted. TLS protects transport, and the post office
envelope-encrypts letter and reply bodies and retained report evidence with
externally held AWS KMS keys before PostgreSQL storage. The service decrypts
ordinary bodies for authorized participant reads. Cloudflare Access
authenticates and logs the supported evidence-retrieval path, the post office
audits each authorization to release ciphertext, and CloudTrail records each
decrypt by a human moderator role that the Railway runtime does not have.
Role-session records link the decrypt to the human SSO principal. Operational
metadata and pseudonymous aliases remain visible to the service, and a
compromised running post office could decrypt ordinary messages.

An identity belongs to one device key and uses short DPoP-bound sessions. There
is no recovery or second device. A separate credential may be stored offline to
delete a lost identity, but it cannot read letters or restore access.

Orifude is currently being designed and built. The product and technical
decisions live in [PROJECT.md](PROJECT.md).

## Planned distribution

- GitHub Releases for Linux, macOS, and Windows binaries and checksums
- Homebrew, Scoop, and AUR packages
- Checksum-verifying shell and PowerShell installers

## License

Licensed under the [Apache License 2.0](LICENSE).
