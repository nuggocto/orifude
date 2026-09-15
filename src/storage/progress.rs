//! Completion transactions, saved replays, and daily history.

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::domain::puzzle::{Puzzle, PuzzleIdentity};
use crate::domain::replay::Replay;
use crate::generator::CalendarDate;
use crate::storage::{
    DecodedReplay, MAX_PAGE_COUNT, PROGRESS_PAGE_QUERY_DB, PROGRESS_PAGE_SIZE, PRUNE_BATCH_DB,
    RECENT_REPLAYS_DB, RESERVE_PAGES, Storage, StorageError, i64_to_u64, replay, u64_to_i64,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PuzzleProgress {
    pub pack_id: Box<str>,
    pub puzzle_id: Box<str>,
    pub attempt_count: u64,
    pub best_folds: u8,
    pub best_strokes: u8,
    pub best_replay_id: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressPage {
    pub entries: Vec<PuzzleProgress>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyHistory {
    pub day: CalendarDate,
    pub generator_version: u16,
    pub pack_id: Box<str>,
    pub puzzle_id: Box<str>,
    pub completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DailyKey {
    pub day: CalendarDate,
    pub generator_version: u16,
}

impl Storage {
    /// Saves one successful attempt, replay, and best-progress update in one
    /// durable transaction.
    ///
    /// # Errors
    ///
    /// Rejects failed or incompatible replays before opening the transaction.
    /// Capacity failure rolls back all three records together.
    pub fn record_completion(
        &mut self,
        puzzle: &Puzzle,
        replay: &Replay,
        completed_at_unix_seconds: i64,
        undo_count: u64,
        hints_used: bool,
    ) -> Result<PuzzleProgress, StorageError> {
        self.record_completion_inner(
            puzzle,
            replay,
            completed_at_unix_seconds,
            undo_count,
            hints_used,
            None,
        )
    }

    /// Saves a successful daily attempt and marks its day complete atomically.
    ///
    /// # Errors
    ///
    /// Uses the same validation and rollback guarantees as [`Self::record_completion`].
    pub fn record_daily_completion(
        &mut self,
        daily: DailyKey,
        puzzle: &Puzzle,
        replay: &Replay,
        completed_at_unix_seconds: i64,
        undo_count: u64,
        hints_used: bool,
    ) -> Result<PuzzleProgress, StorageError> {
        if daily.generator_version == 0 {
            return Err(StorageError::ResourceLimit);
        }
        self.record_completion_inner(
            puzzle,
            replay,
            completed_at_unix_seconds,
            undo_count,
            hints_used,
            Some((daily.day, daily.generator_version)),
        )
    }

    fn record_completion_inner(
        &mut self,
        puzzle: &Puzzle,
        replay: &Replay,
        completed_at_unix_seconds: i64,
        undo_count: u64,
        hints_used: bool,
        daily: Option<(CalendarDate, u16)>,
    ) -> Result<PuzzleProgress, StorageError> {
        let attempt = replay
            .execute(puzzle)
            .map_err(|_| StorageError::InvalidCompletion)?;
        let result = attempt.result();
        if !result.is_success() {
            return Err(StorageError::InvalidCompletion);
        }
        let payload = replay::encode(puzzle, replay).map_err(|_| StorageError::ReplayData)?;
        let score = result.score();
        let record = CompletionRecord {
            puzzle,
            pack_id: puzzle.identity().pack_id(),
            puzzle_id: puzzle.identity().puzzle_id(),
            completed_at: completed_at_unix_seconds,
            folds: score.folds().get(),
            strokes: score.strokes().get(),
            undo_count,
            hints_used,
            payload: &payload,
        };
        self.verify_footprint()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (replay_id, previous, is_best) = insert_completion_rows(&transaction, &record)?;
        let progress = update_progress(&transaction, &record, replay_id, previous, is_best)?;
        if let Some((day, generator_version)) = daily {
            let changed = transaction.execute(
                "INSERT INTO daily_history(day, generator_version, pack_id, puzzle_id, completed)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(day, generator_version) DO UPDATE SET
                   completed = 1
                 WHERE daily_history.pack_id = excluded.pack_id
                   AND daily_history.puzzle_id = excluded.puzzle_id",
                params![
                    day.to_string(),
                    generator_version,
                    record.pack_id,
                    record.puzzle_id,
                ],
            )?;
            if changed != 1 {
                return Err(StorageError::Corrupt);
            }
        }
        prune_puzzle_history(&transaction, record.pack_id, record.puzzle_id)?;
        let reserve_restored = restore_nonessential_reserve(&transaction)?;
        if !reserve_restored && !is_best {
            transaction.execute(
                "DELETE FROM attempts
                 WHERE id = (
                   SELECT attempt_id FROM replays WHERE id = ?1 AND is_best = 0
                 )",
                [replay_id],
            )?;
        }
        transaction.commit()?;
        Ok(progress)
    }

    /// Returns saved progress for one stable puzzle identity.
    ///
    /// # Errors
    ///
    /// Returns a database error without changing state.
    pub fn progress(
        &self,
        pack_id: &str,
        puzzle_id: &str,
    ) -> Result<Option<PuzzleProgress>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT attempt_count, best_folds, best_strokes, best_replay_id, updated_at
                 FROM progress WHERE pack_id = ?1 AND puzzle_id = ?2",
                params![pack_id, puzzle_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, u8>(1)?,
                        row.get::<_, u8>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(attempt_count, best_folds, best_strokes, best_replay_id, updated_at)| {
                Ok(PuzzleProgress {
                    pack_id: pack_id.into(),
                    puzzle_id: puzzle_id.into(),
                    attempt_count: i64_to_u64(attempt_count)?,
                    best_folds,
                    best_strokes,
                    best_replay_id,
                    updated_at_unix_seconds: updated_at,
                })
            },
        )
        .transpose()
    }

    /// Returns one bounded page of saved puzzle summaries, newest first.
    ///
    /// # Errors
    ///
    /// Returns when a row is corrupt or SQLite cannot complete the read.
    pub fn progress_page(&self, offset: u64) -> Result<ProgressPage, StorageError> {
        let offset = i64::try_from(offset).map_err(|_| StorageError::ResourceLimit)?;
        let mut statement = self.connection.prepare(
            "SELECT pack_id, puzzle_id, attempt_count, best_folds, best_strokes,
                    best_replay_id, updated_at
             FROM progress ORDER BY updated_at DESC, pack_id, puzzle_id LIMIT ?1 OFFSET ?2",
        )?;
        let rows = statement.query_map(params![PROGRESS_PAGE_QUERY_DB, offset], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, u8>(3)?,
                row.get::<_, u8>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        let mut progress = Vec::with_capacity(PROGRESS_PAGE_SIZE + 1);
        for row in rows {
            let (pack_id, puzzle_id, attempt_count, folds, strokes, replay_id, updated_at) = row?;
            PuzzleIdentity::new(&pack_id, &puzzle_id).map_err(|_| StorageError::Corrupt)?;
            progress.push(PuzzleProgress {
                pack_id: pack_id.into_boxed_str(),
                puzzle_id: puzzle_id.into_boxed_str(),
                attempt_count: i64_to_u64(attempt_count)?,
                best_folds: folds,
                best_strokes: strokes,
                best_replay_id: replay_id,
                updated_at_unix_seconds: updated_at,
            });
        }
        let has_more = progress.len() > PROGRESS_PAGE_SIZE;
        progress.truncate(PROGRESS_PAGE_SIZE);
        Ok(ProgressPage {
            entries: progress,
            has_more,
        })
    }

    /// Loads and validates the current best replay document.
    ///
    /// # Errors
    ///
    /// Returns when the database or bounded replay document is invalid.
    pub fn best_replay(
        &self,
        pack_id: &str,
        puzzle_id: &str,
    ) -> Result<Option<DecodedReplay>, StorageError> {
        let payload: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT r.payload FROM replays r
                 JOIN progress p ON p.best_replay_id = r.id
                 WHERE p.pack_id = ?1 AND p.puzzle_id = ?2",
                params![pack_id, puzzle_id],
                |row| row.get(0),
            )
            .optional()?;
        let decoded = payload
            .map(|bytes| replay::decode(&bytes).map_err(|_| StorageError::ReplayData))
            .transpose()?;
        if decoded.as_ref().is_some_and(|decoded| {
            decoded.puzzle().identity().pack_id() != pack_id
                || decoded.puzzle().identity().puzzle_id() != puzzle_id
        }) {
            return Err(StorageError::ReplayData);
        }
        Ok(decoded)
    }

    /// Reports whether the saved best replay belongs to this exact gameplay
    /// definition.
    ///
    /// # Errors
    ///
    /// Returns when the database or bounded replay document is invalid.
    pub fn completion_matches(&self, puzzle: &Puzzle) -> Result<bool, StorageError> {
        let identity = puzzle.identity();
        self.best_replay(identity.pack_id(), identity.puzzle_id())
            .map(|saved| saved.is_some_and(|saved| saved.puzzle() == puzzle))
    }

    /// Records an offline daily selection and completion state.
    ///
    /// # Errors
    ///
    /// Returns a typed database error with the prior row intact.
    pub fn record_daily(
        &mut self,
        day: CalendarDate,
        generator_version: u16,
        puzzle: &Puzzle,
        completed: bool,
    ) -> Result<(), StorageError> {
        if generator_version == 0 {
            return Err(StorageError::ResourceLimit);
        }
        self.verify_footprint()?;
        let day = day.to_string();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "INSERT INTO daily_history(day, generator_version, pack_id, puzzle_id, completed)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(day, generator_version) DO UPDATE SET
               completed = daily_history.completed OR excluded.completed
             WHERE daily_history.pack_id = excluded.pack_id
               AND daily_history.puzzle_id = excluded.puzzle_id",
            params![
                day,
                generator_version,
                puzzle.identity().pack_id(),
                puzzle.identity().puzzle_id(),
                completed
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::Corrupt);
        }
        if !restore_nonessential_reserve(&transaction)? {
            return Err(StorageError::Full);
        }
        transaction.commit().map_err(StorageError::from)
    }

    /// Reads one generator-versioned daily selection.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the generator version is invalid or SQLite
    /// cannot read the bounded row.
    pub fn daily_history(
        &self,
        day: CalendarDate,
        generator_version: u16,
    ) -> Result<Option<DailyHistory>, StorageError> {
        if generator_version == 0 {
            return Err(StorageError::ResourceLimit);
        }
        let day_text = day.to_string();
        let history = self
            .connection
            .query_row(
                "SELECT pack_id, puzzle_id, completed FROM daily_history
                 WHERE day = ?1 AND generator_version = ?2",
                params![day_text, generator_version],
                |row| {
                    Ok(DailyHistory {
                        day,
                        generator_version,
                        pack_id: row.get::<_, String>(0)?.into_boxed_str(),
                        puzzle_id: row.get::<_, String>(1)?.into_boxed_str(),
                        completed: row.get(2)?,
                    })
                },
            )
            .optional()?;
        if history.as_ref().is_some_and(|history| {
            PuzzleIdentity::new(&history.pack_id, &history.puzzle_id).is_err()
        }) {
            return Err(StorageError::Corrupt);
        }
        Ok(history)
    }
}

struct CompletionRecord<'a> {
    puzzle: &'a Puzzle,
    pack_id: &'a str,
    puzzle_id: &'a str,
    completed_at: i64,
    folds: u8,
    strokes: u8,
    undo_count: u64,
    hints_used: bool,
    payload: &'a [u8],
}

#[derive(Clone, Copy)]
struct ExistingProgress {
    attempt_count: u64,
    best_folds: u8,
    best_strokes: u8,
    best_replay_id: i64,
}

fn insert_completion_rows(
    transaction: &Transaction<'_>,
    record: &CompletionRecord<'_>,
) -> Result<(i64, Option<ExistingProgress>, bool), StorageError> {
    transaction.execute(
        "INSERT INTO attempts(pack_id, puzzle_id, completed_at, folds, strokes, undo_count, hints_used, success)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
        params![
            record.pack_id,
            record.puzzle_id,
            record.completed_at,
            record.folds,
            record.strokes,
            u64_to_i64(record.undo_count)?,
            record.hints_used
        ],
    )?;
    let attempt_id = transaction.last_insert_rowid();
    let previous = transaction
        .query_row(
            "SELECT attempt_count, best_folds, best_strokes, best_replay_id
             FROM progress WHERE pack_id = ?1 AND puzzle_id = ?2",
            params![record.pack_id, record.puzzle_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, u8>(1)?,
                    row.get::<_, u8>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?
        .map(
            |(count, folds, strokes, replay_id)| -> Result<_, StorageError> {
                Ok(ExistingProgress {
                    attempt_count: i64_to_u64(count)?,
                    best_folds: folds,
                    best_strokes: strokes,
                    best_replay_id: replay_id,
                })
            },
        )
        .transpose()?;
    let is_best = match previous {
        None => true,
        Some(progress) => {
            let payload: Vec<u8> = transaction.query_row(
                "SELECT payload FROM replays WHERE id = ?1",
                [progress.best_replay_id],
                |row| row.get(0),
            )?;
            let saved = replay::decode(&payload).map_err(|_| StorageError::ReplayData)?;
            if saved.puzzle().identity() != record.puzzle.identity() {
                return Err(StorageError::ReplayData);
            }
            // A score only competes with solutions to the same gameplay revision.
            saved.puzzle() != record.puzzle
                || (record.folds, record.strokes) < (progress.best_folds, progress.best_strokes)
        }
    };
    if is_best && let Some(progress) = previous {
        transaction.execute(
            "UPDATE replays SET is_best = 0 WHERE id = ?1",
            [progress.best_replay_id],
        )?;
    }
    transaction.execute(
        "INSERT INTO replays(attempt_id, pack_id, puzzle_id, created_at, payload, is_best)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            attempt_id,
            record.pack_id,
            record.puzzle_id,
            record.completed_at,
            record.payload,
            is_best
        ],
    )?;
    Ok((transaction.last_insert_rowid(), previous, is_best))
}

fn update_progress(
    transaction: &Transaction<'_>,
    record: &CompletionRecord<'_>,
    replay_id: i64,
    previous: Option<ExistingProgress>,
    is_best: bool,
) -> Result<PuzzleProgress, StorageError> {
    let (attempt_count, best_folds, best_strokes, best_replay_id) = match previous {
        Some(progress) if !is_best => (
            progress
                .attempt_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?,
            progress.best_folds,
            progress.best_strokes,
            progress.best_replay_id,
        ),
        Some(progress) => (
            progress
                .attempt_count
                .checked_add(1)
                .ok_or(StorageError::ResourceLimit)?,
            record.folds,
            record.strokes,
            replay_id,
        ),
        None => (1, record.folds, record.strokes, replay_id),
    };
    transaction.execute(
        "INSERT INTO progress(pack_id, puzzle_id, attempt_count, best_folds, best_strokes, best_replay_id, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(pack_id, puzzle_id) DO UPDATE SET
           attempt_count = excluded.attempt_count,
           best_folds = excluded.best_folds,
           best_strokes = excluded.best_strokes,
           best_replay_id = excluded.best_replay_id,
           updated_at = excluded.updated_at",
        params![
            record.pack_id,
            record.puzzle_id,
            u64_to_i64(attempt_count)?,
            best_folds,
            best_strokes,
            best_replay_id,
            record.completed_at
        ],
    )?;
    Ok(PuzzleProgress {
        pack_id: record.pack_id.into(),
        puzzle_id: record.puzzle_id.into(),
        attempt_count,
        best_folds,
        best_strokes,
        best_replay_id,
        updated_at_unix_seconds: record.completed_at,
    })
}

fn prune_puzzle_history(
    transaction: &Transaction<'_>,
    pack_id: &str,
    puzzle_id: &str,
) -> Result<(), StorageError> {
    let mut statement = transaction.prepare(
        "SELECT attempt_id FROM replays
         WHERE pack_id = ?1 AND puzzle_id = ?2
           AND id NOT IN (
             SELECT id FROM replays
             WHERE pack_id = ?1 AND puzzle_id = ?2
             ORDER BY is_best DESC, created_at DESC, id DESC
             LIMIT ?3
           )
         ORDER BY created_at, id LIMIT ?4",
    )?;
    let attempt_ids = statement
        .query_map(
            params![pack_id, puzzle_id, RECENT_REPLAYS_DB, PRUNE_BATCH_DB],
            |row| row.get::<_, i64>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for attempt_id in attempt_ids {
        transaction.execute("DELETE FROM attempts WHERE id = ?1", [attempt_id])?;
    }
    Ok(())
}

fn restore_nonessential_reserve(transaction: &Transaction<'_>) -> Result<bool, StorageError> {
    if available_pages(transaction)? >= RESERVE_PAGES {
        return Ok(true);
    }
    let mut statement = transaction.prepare(
        "SELECT attempt_id FROM replays WHERE is_best = 0
         ORDER BY created_at, id LIMIT ?1",
    )?;
    let attempt_ids = statement
        .query_map([PRUNE_BATCH_DB], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for attempt_id in attempt_ids {
        transaction.execute("DELETE FROM attempts WHERE id = ?1", [attempt_id])?;
    }
    Ok(available_pages(transaction)? >= RESERVE_PAGES)
}

fn available_pages(transaction: &Transaction<'_>) -> Result<u64, StorageError> {
    let page_count: i64 = transaction.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let freelist: i64 = transaction.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    let page_count = i64_to_u64(page_count)?;
    let freelist = i64_to_u64(freelist)?;
    let used = page_count.saturating_sub(freelist);
    Ok(MAX_PAGE_COUNT.saturating_sub(used))
}
