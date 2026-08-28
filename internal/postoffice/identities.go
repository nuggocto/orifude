package postoffice

import (
	"context"
	"errors"

	"github.com/jackc/pgx/v5"
	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/auth"
	"github.com/nuggocto/orifude/internal/database/dbgen"
	"github.com/nuggocto/orifude/internal/textpolicy"
)

func (s *Service) Me(ctx context.Context, principal Principal) (api.GetMeResponse, error) {
	var alias string
	err := s.db.InTx(ctx, func(q *dbgen.Queries) error {
		identity, err := q.LockActiveIdentity(ctx, principal.IdentityID)
		if err != nil {
			return ErrAuthentication
		}
		alias = identity.Alias.String
		return nil
	})
	if err != nil {
		return api.GetMeResponse{}, err
	}
	return api.GetMeResponse{
		Alias: alias, LatestTUIVersion: s.config.LatestTUIVersion,
		Limits: api.Limits{BodyCodePoints: textpolicy.MaxBodyCodePoints, BodyBytes: textpolicy.MaxBodyBytes, RequestBytes: 16 << 10},
	}, nil
}

func (s *Service) DeleteIdentity(ctx context.Context, principal Principal) error {
	return s.db.InTx(ctx, func(q *dbgen.Queries) error {
		identity, err := q.LockActiveIdentity(ctx, principal.IdentityID)
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrAuthentication
		}
		if err != nil {
			return err
		}
		return deleteIdentity(ctx, q, identity)
	})
}

func (s *Service) RevokeIdentity(ctx context.Context, credential string) error {
	if _, ok := decodeSecret(credential); !ok {
		return nil
	}
	hash := auth.HashRevocationCredential(credential)
	return s.db.InTx(ctx, func(q *dbgen.Queries) error {
		identity, err := q.GetIdentityByRevocationHashForUpdate(ctx, hash[:])
		if errors.Is(err, pgx.ErrNoRows) {
			return nil
		}
		if err != nil {
			return err
		}
		return deleteIdentity(ctx, q, identity)
	})
}

func deleteIdentity(ctx context.Context, q *dbgen.Queries, identity dbgen.Identity) error {
	if identity.DeletedAt.Valid {
		return nil
	}
	operations := []func() error{
		func() error { _, err := q.ReserveIdentityAlias(ctx, identity.ID); return err },
		func() error { _, err := q.ReleaseIdentityUnopenedClaims(ctx, int8(identity.ID)); return err },
		func() error { _, err := q.DeleteIdentityWaitingLetters(ctx, identity.ID); return err },
		func() error { _, err := q.RemoveIdentityKeepsakes(ctx, identity.ID); return err },
		func() error { _, err := q.DeleteFullyRemovedIdentityLetters(ctx, identity.ID); return err },
		func() error { _, err := q.DeleteIdentityBlocks(ctx, identity.ID); return err },
		func() error { _, err := q.RevokeIdentitySessions(ctx, identity.ID); return err },
		func() error { _, err := q.MarkIdentityDeleted(ctx, identity.ID); return err },
	}
	for _, operation := range operations {
		if err := operation(); err != nil {
			return err
		}
	}
	return nil
}
