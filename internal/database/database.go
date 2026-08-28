package database

import (
	"context"
	"errors"
	"fmt"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/nuggocto/orifude/internal/database/dbgen"
)

type DB struct {
	pool *pgxpool.Pool
}

func Open(ctx context.Context, databaseURL string, maxConns int32) (*DB, error) {
	if maxConns < 1 {
		return nil, errors.New("database max connections must be positive")
	}

	config, err := pgxpool.ParseConfig(databaseURL)
	if err != nil {
		return nil, fmt.Errorf("parse database configuration: %w", err)
	}
	config.MaxConns = maxConns

	pool, err := pgxpool.NewWithConfig(ctx, config)
	if err != nil {
		return nil, fmt.Errorf("create database pool: %w", err)
	}
	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("ping database: %w", err)
	}

	return &DB{pool: pool}, nil
}

func (db *DB) Ready(ctx context.Context) error {
	return db.pool.Ping(ctx)
}

func (db *DB) Queries() *dbgen.Queries {
	return dbgen.New(db.pool)
}

func (db *DB) InTx(ctx context.Context, fn func(*dbgen.Queries) error) error {
	return pgx.BeginTxFunc(ctx, db.pool, pgx.TxOptions{IsoLevel: pgx.ReadCommitted}, func(tx pgx.Tx) error {
		return fn(dbgen.New(tx))
	})
}

func (db *DB) Close() {
	db.pool.Close()
}
