use std::error::Error;
use std::fmt;

use time::OffsetDateTime;
#[cfg(test)]
use time::UtcOffset;

use crate::generator::{CalendarDate, CalendarDateError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClockSnapshot {
    pub(crate) date: CalendarDate,
    pub(crate) unix_seconds: i64,
}

pub(crate) struct Clock {
    mode: ClockMode,
}

enum ClockMode {
    Local,
    #[cfg(any(feature = "isolated-test-paths", test))]
    Fixed(ClockSnapshot),
}

impl Clock {
    pub(crate) fn runtime() -> Result<Self, ClockError> {
        #[cfg(feature = "isolated-test-paths")]
        if let Some(snapshot) = injected_snapshot()? {
            return Ok(Self {
                mode: ClockMode::Fixed(snapshot),
            });
        }

        let clock = Self {
            mode: ClockMode::Local,
        };
        let _ = clock.now()?;
        Ok(clock)
    }

    pub(crate) fn now(&self) -> Result<ClockSnapshot, ClockError> {
        match self.mode {
            #[cfg(any(feature = "isolated-test-paths", test))]
            ClockMode::Fixed(snapshot) => Ok(snapshot),
            ClockMode::Local => {
                snapshot(OffsetDateTime::now_local().map_err(|_| ClockError::LocalOffset)?)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockError {
    LocalOffset,
    InvalidDate(CalendarDateError),
    InvalidInjectedValue,
}

impl fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalOffset => formatter.write_str("the local calendar offset is unavailable"),
            Self::InvalidDate(_) => formatter.write_str("the local calendar date is invalid"),
            Self::InvalidInjectedValue => {
                formatter.write_str("the isolated test clock is malformed")
            }
        }
    }
}

impl Error for ClockError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidDate(error) => Some(error),
            Self::LocalOffset | Self::InvalidInjectedValue => None,
        }
    }
}

fn snapshot(now: OffsetDateTime) -> Result<ClockSnapshot, ClockError> {
    let year = u16::try_from(now.year()).map_err(|_| ClockError::LocalOffset)?;
    let date = CalendarDate::new(year, u8::from(now.month()), now.day())
        .map_err(ClockError::InvalidDate)?;
    Ok(ClockSnapshot {
        date,
        unix_seconds: now.unix_timestamp(),
    })
}

#[cfg(feature = "isolated-test-paths")]
fn injected_snapshot() -> Result<Option<ClockSnapshot>, ClockError> {
    let date = std::env::var("ORIFUDE_TEST_DATE").ok();
    let unix_seconds = std::env::var("ORIFUDE_TEST_UNIX_SECONDS").ok();
    match (date, unix_seconds) {
        (None, None) => Ok(None),
        (Some(date), Some(unix_seconds)) => {
            let mut parts = date.split('-');
            let year = parts.next().and_then(|value| value.parse::<u16>().ok());
            let month = parts.next().and_then(|value| value.parse::<u8>().ok());
            let day = parts.next().and_then(|value| value.parse::<u8>().ok());
            if parts.next().is_some() {
                return Err(ClockError::InvalidInjectedValue);
            }
            let date = CalendarDate::new(
                year.ok_or(ClockError::InvalidInjectedValue)?,
                month.ok_or(ClockError::InvalidInjectedValue)?,
                day.ok_or(ClockError::InvalidInjectedValue)?,
            )
            .map_err(ClockError::InvalidDate)?;
            let unix_seconds = unix_seconds
                .parse()
                .map_err(|_| ClockError::InvalidInjectedValue)?;
            Ok(Some(ClockSnapshot { date, unix_seconds }))
        }
        (Some(_), None) | (None, Some(_)) => Err(ClockError::InvalidInjectedValue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_returns_the_same_injected_day_and_timestamp() {
        let snapshot = ClockSnapshot {
            date: CalendarDate::new(2026, 9, 2).expect("date"),
            unix_seconds: 1_777_777_777,
        };
        let clock = Clock {
            mode: ClockMode::Fixed(snapshot),
        };

        assert_eq!(clock.now().expect("fixed clock"), snapshot);
    }

    #[test]
    fn snapshots_follow_the_offset_of_each_local_observation() {
        let instant = OffsetDateTime::from_unix_timestamp(1_767_225_000).expect("timestamp");
        let west = snapshot(instant.to_offset(UtcOffset::from_hms(-1, 0, 0).expect("offset")))
            .expect("western snapshot");
        let east = snapshot(instant.to_offset(UtcOffset::from_hms(1, 0, 0).expect("offset")))
            .expect("eastern snapshot");

        assert_ne!(west.date, east.date);
        assert_eq!(west.unix_seconds, east.unix_seconds);
    }
}
