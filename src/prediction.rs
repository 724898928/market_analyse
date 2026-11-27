use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::deriv::moving_average;
use crate::market::KlineBar;

const DB_ENV: &str = "PREDICTION_DB";
const DEFAULT_DB: &str = "predictions.db";
const MAX_BARS: usize = 120;

pub type SeriesInput<'a> = (&'a str, usize);

#[derive(Serialize)]
pub struct PredictionHistory {
    pub total: usize,
    pub evaluated: usize,
    pub successes: usize,
    pub success_rate: Option<f64>,
    pub mae: Option<f64>,
    pub rmse: Option<f64>,
    pub by_series: Vec<GroupStat>,
    pub by_horizon: Vec<GroupStat>,
    pub points: Vec<HistoryPoint>,
}

#[derive(Serialize)]
pub struct GroupStat {
    pub name: String,
    pub evaluated: usize,
    pub successes: usize,
    pub success_rate: Option<f64>,
    pub mae: Option<f64>,
}

#[derive(Serialize)]
pub struct HistoryPoint {
    pub time_ms: i64,
    pub date: String,
    pub ma5_pred: Option<f64>,
    pub ma5_actual: Option<f64>,
    pub ma10_pred: Option<f64>,
    pub ma10_actual: Option<f64>,
    pub ma20_pred: Option<f64>,
    pub ma20_actual: Option<f64>,
}

#[derive(Serialize)]
pub struct WatchItem {
    pub symbol: String,
    pub name: String,
    pub created_at: String,
    pub analyzed_at: String,
    pub hits: i64,
}

struct RowData {
    series: String,
    horizon: i64,
    base_value: f64,
    predicted: f64,
    actual: f64,
    abs_error: f64,
    success: Option<i64>,
    time_ms: i64,
    date: String,
}

fn db_path() -> String {
    env::var(DB_ENV).unwrap_or_else(|_| DEFAULT_DB.to_string())
}

fn open_db() -> Result<Connection, String> {
    let conn = Connection::open(db_path()).map_err(|error| error.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS predictions (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             symbol TEXT NOT NULL,
             period TEXT NOT NULL,
             source TEXT NOT NULL,
             mode TEXT NOT NULL,
             series TEXT NOT NULL,
             base_time_ms INTEGER NOT NULL,
             base_date TEXT NOT NULL,
             base_value REAL NOT NULL,
             horizon INTEGER NOT NULL,
             delta REAL NOT NULL,
             predicted REAL NOT NULL,
             target_time_ms INTEGER NOT NULL,
             target_date TEXT NOT NULL,
             actual REAL,
             absolute_error REAL,
             success INTEGER,
             created_at TEXT NOT NULL,
             UNIQUE(symbol, period, source, mode, series, base_time_ms, horizon, target_time_ms)
         );
         CREATE INDEX IF NOT EXISTS idx_predictions_lookup
             ON predictions(symbol, period, source, mode, target_time_ms);
         CREATE TABLE IF NOT EXISTS watchlist (
             symbol TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             created_at TEXT NOT NULL,
             analyzed_at TEXT NOT NULL,
             hits INTEGER NOT NULL DEFAULT 0
         );",
    )
    .map_err(|error| error.to_string())?;
    Ok(conn)
}

fn typical_step(x_axis: &[f64]) -> f64 {
    if x_axis.len() < 2 {
        return 1.0;
    }
    let start = x_axis.len().saturating_sub(21);
    let mut gaps: Vec<f64> = (start + 1..x_axis.len())
        .map(|index| x_axis[index] - x_axis[index - 1])
        .filter(|gap| gap.is_finite() && *gap > 0.0)
        .collect();
    if gaps.is_empty() {
        return 1.0;
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    gaps[gaps.len() / 2]
}

fn direction(value: f64) -> i8 {
    if value > 1e-9 {
        1
    } else if value < -1e-9 {
        -1
    } else {
        0
    }
}

fn success_of(base: f64, predicted: f64, actual: f64) -> i64 {
    if direction(predicted - base) == direction(actual - base) {
        1
    } else {
        0
    }
}

fn causal_change(bars: &[KlineBar], ma: &[Option<f64>], index: usize, delta: f64) -> Option<f64> {
    if index == 0 {
        return None;
    }
    let current = ma[index]?;
    let previous = ma[index - 1]?;
    if !current.is_finite() || !previous.is_finite() {
        return None;
    }
    let ma_change = current - previous;
    let close_change = bars[index].close - bars[index - 1].close;
    let damping = if ma_change * close_change > 0.0 {
        1.0
    } else {
        0.3
    };
    Some(ma_change * delta * damping)
}

pub fn record_predictions(
    symbol: &str,
    period: &str,
    source: &str,
    mode: &str,
    bars: &[KlineBar],
    x_axis: &[f64],
    series: &[SeriesInput<'_>],
) -> Result<(), String> {
    if bars.len() < 3 || bars.len() != x_axis.len() || series.is_empty() {
        return Ok(());
    }

    let mut conn = open_db()?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM predictions WHERE symbol=?1 AND period=?2 AND source=?3 AND mode=?4 AND target_time_ms=0",
            params![symbol, period, source, mode],
        )
        .map_err(|error| error.to_string())?;
    let closes: Vec<f64> = bars.iter().map(|bar| bar.close).collect();
    let mas: Vec<Vec<Option<f64>>> = series
        .iter()
        .map(|(_, window)| moving_average(&closes, *window))
        .collect();
    let step = typical_step(x_axis);
    let created_at = Utc::now().to_rfc3339();
    let start = bars.len().saturating_sub(MAX_BARS);

    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO predictions (
                     symbol, period, source, mode, series, base_time_ms, base_date,
                     base_value, horizon, delta, predicted, target_time_ms, target_date,
                     actual, absolute_error, success, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                 ON CONFLICT(symbol, period, source, mode, series, base_time_ms, horizon, target_time_ms)
                 DO UPDATE SET
                     base_value=excluded.base_value, delta=excluded.delta,
                     predicted=excluded.predicted, target_date=excluded.target_date,
                     actual=excluded.actual, absolute_error=excluded.absolute_error,
                     success=excluded.success, created_at=excluded.created_at",
            )
            .map_err(|error| error.to_string())?;

        for index in start..bars.len() {
            for (series_index, (name, _)) in series.iter().enumerate() {
                let base_value = mas[series_index][index];
                if !matches!(base_value, Some(value) if value.is_finite()) {
                    continue;
                }
                let base_value = base_value.unwrap();
                for horizon in 1..=3i64 {
                    let target_index = index + horizon as usize;
                    let (delta, target_time, target_date, actual) = if target_index < bars.len() {
                        (
                            x_axis[target_index] - x_axis[index],
                            bars[target_index].time_ms,
                            bars[target_index].label.as_str(),
                            mas[series_index][target_index],
                        )
                    } else if index == bars.len() - 1 {
                        (step * horizon as f64, 0, "", None)
                    } else {
                        continue;
                    };
                    if !delta.is_finite() {
                        continue;
                    }
                    let change = match causal_change(bars, &mas[series_index], index, delta) {
                        Some(value) => value,
                        None => continue,
                    };
                    let predicted = base_value + change;
                    if !predicted.is_finite() {
                        continue;
                    }
                    let (absolute_error, success) = match actual {
                        Some(value) if value.is_finite() => (
                            Some((predicted - value).abs()),
                            Some(success_of(base_value, predicted, value)),
                        ),
                        _ => (None, None),
                    };

                    statement
                        .execute(params![
                            symbol,
                            period,
                            source,
                            mode,
                            name,
                            bars[index].time_ms,
                            bars[index].label.as_str(),
                            base_value,
                            horizon,
                            delta,
                            predicted,
                            target_time,
                            target_date,
                            actual,
                            absolute_error,
                            success,
                            created_at,
                        ])
                        .map_err(|error| error.to_string())?;
                }
            }
        }
    }

    transaction.commit().map_err(|error| error.to_string())
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut count = 0usize;
    let mut sum = 0.0;
    for value in values {
        sum += value;
        count += 1;
    }
    (count > 0).then_some(sum / count as f64)
}

fn stat_from(name: String, rows: &[&RowData]) -> GroupStat {
    let evaluated = rows.len();
    let successes = rows
        .iter()
        .filter(|row| {
            row.success
                .unwrap_or_else(|| success_of(row.base_value, row.predicted, row.actual))
                == 1
        })
        .count();
    GroupStat {
        name,
        evaluated,
        successes,
        success_rate: (evaluated > 0).then_some(successes as f64 / evaluated as f64 * 100.0),
        mae: mean(rows.iter().map(|row| row.abs_error)),
    }
}

pub fn history(
    symbol: &str,
    period: Option<&str>,
    source: Option<&str>,
    mode: Option<&str>,
    limit: usize,
) -> Result<PredictionHistory, String> {
    let conn = open_db()?;
    let mut statement = conn
        .prepare(
            "SELECT series, horizon, base_value, predicted, actual, absolute_error,
                    success, target_time_ms, target_date
             FROM predictions
             WHERE symbol=?1
               AND (?2 IS NULL OR period=?2)
               AND (?3 IS NULL OR source=?3)
               AND (?4 IS NULL OR mode=?4)
               AND actual IS NOT NULL
             ORDER BY target_time_ms, series, horizon",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![symbol, period, source, mode], |row| {
            Ok(RowData {
                series: row.get(0)?,
                horizon: row.get(1)?,
                base_value: row.get(2)?,
                predicted: row.get(3)?,
                actual: row.get(4)?,
                abs_error: row.get(5)?,
                success: row.get(6)?,
                time_ms: row.get(7)?,
                date: row.get(8)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let row_refs: Vec<&RowData> = rows.iter().collect();
    let evaluated = rows.len();
    let successes = row_refs
        .iter()
        .filter(|row| {
            row.success
                .unwrap_or_else(|| success_of(row.base_value, row.predicted, row.actual))
                == 1
        })
        .count();
    let mae = mean(rows.iter().map(|row| row.abs_error));
    let rmse = mean(rows.iter().map(|row| row.abs_error * row.abs_error)).map(|value| value.sqrt());

    let mut by_series: BTreeMap<String, Vec<&RowData>> = BTreeMap::new();
    let mut by_horizon: BTreeMap<i64, Vec<&RowData>> = BTreeMap::new();
    for row in &row_refs {
        by_series.entry(row.series.clone()).or_default().push(row);
        by_horizon.entry(row.horizon).or_default().push(row);
    }
    let by_series = by_series
        .into_iter()
        .map(|(name, rows)| stat_from(name, &rows))
        .collect();
    let by_horizon = by_horizon
        .into_iter()
        .map(|(horizon, rows)| stat_from(format!("未来{horizon}根"), &rows))
        .collect();

    let mut grouped: BTreeMap<i64, BTreeMap<String, Vec<&RowData>>> = BTreeMap::new();
    for row in &row_refs {
        grouped
            .entry(row.time_ms)
            .or_default()
            .entry(row.series.clone())
            .or_default()
            .push(row);
    }
    let mut points: Vec<HistoryPoint> = grouped
        .into_iter()
        .map(|(time_ms, by_series)| {
            let date = by_series
                .values()
                .next()
                .and_then(|rows| rows.first())
                .map(|row| row.date.clone())
                .unwrap_or_default();
            let mut point = HistoryPoint {
                time_ms,
                date,
                ma5_pred: None,
                ma5_actual: None,
                ma10_pred: None,
                ma10_actual: None,
                ma20_pred: None,
                ma20_actual: None,
            };
            for (name, rows) in by_series {
                let predicted = mean(rows.iter().map(|row| row.predicted)).unwrap_or(0.0);
                let actual = mean(rows.iter().map(|row| row.actual)).unwrap_or(0.0);
                match name.as_str() {
                    "ma5" => {
                        point.ma5_pred = Some(predicted);
                        point.ma5_actual = Some(actual);
                    }
                    "ma10" => {
                        point.ma10_pred = Some(predicted);
                        point.ma10_actual = Some(actual);
                    }
                    "ma20" => {
                        point.ma20_pred = Some(predicted);
                        point.ma20_actual = Some(actual);
                    }
                    _ => {}
                }
            }
            point
        })
        .collect();
    let skip = points.len().saturating_sub(limit);
    points = points.into_iter().skip(skip).collect();

    drop(statement);
    let mut total_statement = conn
        .prepare(
            "SELECT COUNT(*) FROM predictions WHERE symbol=?1 AND (?2 IS NULL OR period=?2) AND (?3 IS NULL OR source=?3) AND (?4 IS NULL OR mode=?4)",
        )
        .map_err(|error| error.to_string())?;
    let total = total_statement
        .query_row(params![symbol, period, source, mode], |row| {
            row.get::<_, i64>(0)
        })
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or(0) as usize;

    Ok(PredictionHistory {
        total,
        evaluated,
        successes,
        success_rate: (evaluated > 0).then_some(successes as f64 / evaluated as f64 * 100.0),
        mae,
        rmse,
        by_series,
        by_horizon,
        points,
    })
}

fn clean_watch_input(symbol: &str, name: &str) -> (String, String) {
    let symbol = symbol.trim().to_string();
    let name = name.trim();
    let name = if name.is_empty() {
        symbol.clone()
    } else {
        name.to_string()
    };
    (symbol, name)
}

pub fn record_analysis(symbol: &str, name: &str) -> Result<(), String> {
    if symbol.trim().is_empty() || symbol.eq_ignore_ascii_case("demo") {
        return Ok(());
    }
    let (symbol, name) = clean_watch_input(symbol, name);
    let conn = open_db()?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO watchlist (symbol, name, created_at, analyzed_at, hits)
         VALUES (?1, ?2, ?3, ?3, 1)
         ON CONFLICT(symbol) DO UPDATE SET
             name=excluded.name, analyzed_at=excluded.analyzed_at, hits=watchlist.hits+1",
        params![symbol, name, now],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn add_watch(symbol: &str, name: &str) -> Result<(), String> {
    let (symbol, name) = clean_watch_input(symbol, name);
    if symbol.is_empty() {
        return Err("请输入股票代码".to_string());
    }
    let conn = open_db()?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO watchlist (symbol, name, created_at, analyzed_at, hits)
         VALUES (?1, ?2, ?3, ?3, 0)
         ON CONFLICT(symbol) DO UPDATE SET name=excluded.name",
        params![symbol, name, now],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn remove_watch(symbol: &str) -> Result<(), String> {
    let symbol = symbol.trim();
    if symbol.is_empty() {
        return Ok(());
    }
    let conn = open_db()?;
    conn.execute("DELETE FROM watchlist WHERE symbol=?1", params![symbol])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_watch() -> Result<Vec<WatchItem>, String> {
    let conn = open_db()?;
    let mut statement = conn
        .prepare(
            "SELECT symbol, name, created_at, analyzed_at, hits
             FROM watchlist
             ORDER BY analyzed_at DESC, created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| {
            Ok(WatchItem {
                symbol: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                analyzed_at: row.get(3)?,
                hits: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_classifies_signs() {
        assert_eq!(direction(0.5), 1);
        assert_eq!(direction(-0.5), -1);
        assert_eq!(direction(0.0), 0);
    }

    #[test]
    fn success_matches_direction() {
        assert_eq!(success_of(10.0, 10.5, 11.0), 1);
        assert_eq!(success_of(10.0, 10.5, 9.0), 0);
        assert_eq!(success_of(10.0, 10.0, 10.0), 1);
    }

    #[test]
    fn typical_step_uses_median() {
        assert_eq!(typical_step(&[0.0, 1.0, 2.0, 9.0, 10.0]), 1.0);
    }
}
