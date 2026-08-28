package postoffice

import (
	"bytes"
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"io"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database/dbgen"
	"github.com/nuggocto/orifude/internal/textpolicy"
)

const (
	challengeRegistration int16 = 1
	challengeSession      int16 = 2
)

func (s *Service) CreateChallenge(ctx context.Context, request api.CreateChallengeRequest) (api.CreateChallengeResponse, error) {
	encoded, err := json.Marshal(request.PublicJWK)
	if err != nil {
		return api.CreateChallengeResponse{}, ErrInvalid
	}
	key, err := auth.ParsePublicJWK(encoded)
	if err != nil {
		return api.CreateChallengeResponse{}, ErrInvalid
	}

	var challenge auth.Challenge
	if err := s.randomCall(func(random io.Reader) error {
		var randomErr error
		challenge, randomErr = auth.NewChallenge(random)
		return randomErr
	}); err != nil {
		return api.CreateChallengeResponse{}, err
	}

	purpose := challengeRegistration
	identityID := pgtype.Int8{}
	switch request.Purpose {
	case api.ChallengePurposeRegistration:
	case api.ChallengePurposeSession:
		purpose = challengeSession
		identity, lookupErr := s.db.Queries().GetActiveIdentityByThumbprint(ctx, key.Thumbprint[:])
		if errors.Is(lookupErr, pgx.ErrNoRows) {
			now := s.config.Now().UTC()
			return api.CreateChallengeResponse{ChallengeID: challenge.ID, Nonce: challenge.Nonce, ExpiresIn: 300, ServerTime: now}, nil
		}
		if lookupErr != nil {
			return api.CreateChallengeResponse{}, lookupErr
		}
		identityID = int8(identity.ID)
	default:
		return api.CreateChallengeResponse{}, ErrInvalid
	}

	created, err := s.db.Queries().CreateAuthChallenge(ctx, dbgen.CreateAuthChallengeParams{
		ID: challenge.ID, IdentityID: identityID, PublicKey: key.Uncompressed,
		KeyThumbprint: key.Thumbprint[:], Purpose: purpose, NonceHash: challenge.NonceHash[:],
	})
	if err != nil {
		return api.CreateChallengeResponse{}, err
	}
	return api.CreateChallengeResponse{
		ChallengeID: created.ID,
		Nonce:       challenge.Nonce,
		ExpiresIn:   300,
		ServerTime:  created.CreatedAt.Time,
	}, nil
}

func (s *Service) Register(ctx context.Context, request api.CreateIdentityRequest, proof string) (api.CreateIdentityResponse, error) {
	alias, aliasKey, err := textpolicy.NormalizeAlias(request.Alias)
	if err != nil {
		return api.CreateIdentityResponse{}, errors.Join(ErrAliasInvalid, err)
	}
	revocationHash, ok := decodeSecret(request.RevocationHash)
	if !ok || !validID(request.ChallengeID) {
		return api.CreateIdentityResponse{}, ErrInvalid
	}
	inviteHash, inviteOK := inviteTokenHash(request.InviteCode)
	if s.config.InviteRequired && !inviteOK {
		return api.CreateIdentityResponse{}, ErrInviteInvalid
	}

	token, tokenHash, err := s.newAccessToken()
	if err != nil {
		return api.CreateIdentityResponse{}, err
	}
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		challenge, err := q.GetAuthChallengeForUpdate(ctx, request.ChallengeID)
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrAuthentication
		}
		if err != nil {
			return err
		}
		if challenge.Purpose != challengeRegistration || len(challenge.NonceHash) != sha256.Size || len(challenge.KeyThumbprint) != sha256.Size {
			return ErrAuthentication
		}
		var nonceHash, thumbprint [sha256.Size]byte
		copy(nonceHash[:], challenge.NonceHash)
		copy(thumbprint[:], challenge.KeyThumbprint)
		if _, err := s.verifier.VerifyChallenge(auth.ChallengeProofParams{
			Proof: proof, Method: "POST", EscapedPath: "/v1/identities", NonceHash: nonceHash,
			KeyThumbprint: thumbprint, Now: s.config.Now(),
		}); err != nil {
			return errors.Join(ErrAuthentication, err)
		}
		if _, err := q.ConsumeAuthChallenge(ctx, dbgen.ConsumeAuthChallengeParams{ID: request.ChallengeID, Purpose: challengeRegistration}); err != nil {
			if errors.Is(err, pgx.ErrNoRows) {
				return ErrAuthentication
			}
			return err
		}
		if err := q.LockRegistrationKey(ctx, challenge.KeyThumbprint); err != nil {
			return err
		}

		identity, err := q.GetActiveIdentityByPublicKey(ctx, challenge.PublicKey)
		if errors.Is(err, pgx.ErrNoRows) {
			if s.config.InviteRequired {
				if _, err := q.GetRedeemableInviteForUpdate(ctx, inviteHash[:]); err != nil {
					if errors.Is(err, pgx.ErrNoRows) {
						return ErrInviteInvalid
					}
					return err
				}
			}
			identity, err = q.CreateIdentity(ctx, dbgen.CreateIdentityParams{
				PublicKey: challenge.PublicKey, KeyThumbprint: challenge.KeyThumbprint,
				RevocationHash: revocationHash, Alias: text(alias), AliasKey: aliasKey,
			})
			if errors.Is(err, pgx.ErrNoRows) || isUniqueViolation(err) {
				return ErrIdentityConflict
			}
			if err != nil {
				return err
			}
			if s.config.InviteRequired {
				if _, err := q.RedeemInvite(ctx, dbgen.RedeemInviteParams{IdentityID: int8(identity.ID), TokenHash: inviteHash[:]}); err != nil {
					if errors.Is(err, pgx.ErrNoRows) {
						return ErrInviteInvalid
					}
					return err
				}
			}
		} else if err != nil {
			return err
		} else if identity.Alias.String != alias || subtle.ConstantTimeCompare(identity.RevocationHash, revocationHash) != 1 {
			return ErrIdentityConflict
		} else if s.config.InviteRequired {
			invite, err := q.GetInviteForUpdate(ctx, inviteHash[:])
			if err != nil || !invite.RedeemedBy.Valid || invite.RedeemedBy.Int64 != identity.ID {
				return ErrIdentityConflict
			}
		}

		_, err = q.CreateAccessSession(ctx, dbgen.CreateAccessSessionParams{
			TokenHash: tokenHash[:], IdentityID: identity.ID, KeyThumbprint: identity.KeyThumbprint,
		})
		return err
	})
	if err != nil {
		return api.CreateIdentityResponse{}, err
	}
	return api.CreateIdentityResponse{TokenType: api.TokenTypeDPoP, AccessToken: token, ExpiresIn: 900}, nil
}

func (s *Service) CreateSession(ctx context.Context, request api.CreateSessionRequest, proof string) (api.CreateSessionResponse, error) {
	if !validID(request.ChallengeID) {
		return api.CreateSessionResponse{}, ErrInvalid
	}
	token, tokenHash, err := s.newAccessToken()
	if err != nil {
		return api.CreateSessionResponse{}, err
	}
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		challenge, err := q.GetAuthChallengeForUpdate(ctx, request.ChallengeID)
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrAuthentication
		}
		if err != nil {
			return err
		}
		if challenge.Purpose != challengeSession || !challenge.IdentityID.Valid || len(challenge.NonceHash) != sha256.Size || len(challenge.KeyThumbprint) != sha256.Size {
			return ErrAuthentication
		}
		var nonceHash, thumbprint [sha256.Size]byte
		copy(nonceHash[:], challenge.NonceHash)
		copy(thumbprint[:], challenge.KeyThumbprint)
		if _, err := s.verifier.VerifyChallenge(auth.ChallengeProofParams{
			Proof: proof, Method: "POST", EscapedPath: "/v1/sessions", NonceHash: nonceHash,
			KeyThumbprint: thumbprint, Now: s.config.Now(),
		}); err != nil {
			if errors.Is(err, auth.ErrProofClockSkew) {
				return errors.Join(ErrAuthentication, auth.ErrProofClockSkew)
			}
			return ErrAuthentication
		}
		if _, err := q.ConsumeAuthChallenge(ctx, dbgen.ConsumeAuthChallengeParams{ID: request.ChallengeID, Purpose: challengeSession}); err != nil {
			if errors.Is(err, pgx.ErrNoRows) {
				return ErrAuthentication
			}
			return err
		}
		identity, err := q.LockActiveIdentity(ctx, challenge.IdentityID.Int64)
		if err != nil || !bytes.Equal(identity.KeyThumbprint, challenge.KeyThumbprint) {
			return ErrAuthentication
		}
		_, err = q.CreateAccessSession(ctx, dbgen.CreateAccessSessionParams{
			TokenHash: tokenHash[:], IdentityID: identity.ID, KeyThumbprint: identity.KeyThumbprint,
		})
		return err
	})
	if err != nil {
		return api.CreateSessionResponse{}, err
	}
	return api.CreateSessionResponse{TokenType: api.TokenTypeDPoP, AccessToken: token, ExpiresIn: 900}, nil
}

func (s *Service) Authenticate(ctx context.Context, accessToken, proof, method, escapedPath string) (Principal, error) {
	if _, ok := decodeSecret(accessToken); !ok {
		return Principal{}, ErrAuthentication
	}
	tokenHash := auth.HashAccessToken(accessToken)
	session, err := s.db.Queries().GetActiveAccessSession(ctx, tokenHash[:])
	if errors.Is(err, pgx.ErrNoRows) {
		expired, lookupErr := s.db.Queries().AccessSessionExpired(ctx, tokenHash[:])
		if lookupErr != nil {
			return Principal{}, lookupErr
		}
		if expired {
			return Principal{}, ErrSessionExpired
		}
		return Principal{}, ErrAuthentication
	}
	if err != nil {
		return Principal{}, err
	}
	if len(session.KeyThumbprint) != sha256.Size {
		return Principal{}, ErrAuthentication
	}
	var thumbprint [sha256.Size]byte
	copy(thumbprint[:], session.KeyThumbprint)
	verified, err := s.verifier.VerifyResource(auth.ResourceProofParams{
		Proof: proof, Method: method, EscapedPath: escapedPath, AccessToken: accessToken,
		KeyThumbprint: thumbprint, Now: s.config.Now(),
	})
	if err != nil {
		return Principal{}, errors.Join(ErrAuthentication, err)
	}
	err = s.db.InTx(ctx, func(q *dbgen.Queries) error {
		rows, err := q.InsertDPoPReplay(ctx, dbgen.InsertDPoPReplayParams{JtiHash: verified.JTIHash[:], SessionTokenHash: tokenHash[:]})
		if isUniqueViolation(err) {
			return ErrReplay
		}
		if err != nil {
			return err
		}
		if rows != 1 {
			expired, err := q.AccessSessionExpired(ctx, tokenHash[:])
			if err != nil {
				return err
			}
			if expired {
				return ErrSessionExpired
			}
			return ErrAuthentication
		}
		rows, err = q.TouchIdentity(ctx, session.IdentityID)
		if err != nil {
			return err
		}
		if rows != 1 {
			return ErrAuthentication
		}
		return nil
	})
	if err != nil {
		return Principal{}, err
	}
	return Principal{IdentityID: session.IdentityID}, nil
}

func (s *Service) newAccessToken() (string, [sha256.Size]byte, error) {
	var token string
	var hash [sha256.Size]byte
	err := s.randomCall(func(random io.Reader) error {
		var err error
		token, hash, err = auth.NewAccessToken(random)
		return err
	})
	return token, hash, err
}

func inviteTokenHash(value string) ([sha256.Size]byte, bool) {
	if _, ok := decodeSecret(value); !ok {
		return [sha256.Size]byte{}, false
	}
	return auth.HashOpaque(value), true
}
