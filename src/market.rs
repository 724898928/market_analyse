use std::collections::HashMap;
use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Min1,
    Min5,
    Min15,
    Min30,
    Min60,
    Day,
    Week,
    Month,
}

impl Period {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "1m" | "min1" | "minute" => Ok(Self::Min1),
            "5" | "5m" | "min5" => Ok(Self::Min5),
            "15" | "15m" | "min15" => Ok(Self::Min15),
            "30" | "30m" | "min30" => Ok(Self::Min30),
            "60" | "60m" | "min60" => Ok(Self::Min60),
            "101" | "d" | "day" | "daily" => Ok(Self::Day),
            "102" | "w" | "week" | "weekly" => Ok(Self::Week),
            "103" | "mo" | "month" | "monthly" => Ok(Self::Month),
            _ => Err(format!("不支持的K线周期: {value}")),
        }
    }

    pub fn klt(self) -> &'static str {
        match self {
            Self::Min1 => "1",
            Self::Min5 => "5",
            Self::Min15 => "15",
            Self::Min30 => "30",
            Self::Min60 => "60",
            Self::Day => "101",
            Self::Week => "102",
            Self::Month => "103",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Min1 => "1分钟",
            Self::Min5 => "5分钟",
            Self::Min15 => "15分钟",
            Self::Min30 => "30分钟",
            Self::Min60 => "60分钟",
            Self::Day => "日K",
            Self::Week => "周K",
            Self::Month => "月K",
        }
    }

    pub fn is_intraday(self) -> bool {
        matches!(
            self,
            Self::Min1 | Self::Min5 | Self::Min15 | Self::Min30 | Self::Min60
        )
    }

    pub fn time_unit(self) -> &'static str {
        if self.is_intraday() { "分钟" } else { "天" }
    }

    pub fn time_divisor(self) -> f64 {
        if self.is_intraday() { 60.0 } else { 86_400.0 }
    }

    pub fn minutes(self) -> u32 {
        match self {
            Self::Min1 => 1,
            Self::Min5 => 5,
            Self::Min15 => 15,
            Self::Min30 => 30,
            Self::Min60 => 60,
            Self::Day | Self::Week | Self::Month => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Market {
    AShare,
    HongKong,
    Us,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    Auto,
    Futu,
    Tencent,
    Eastmoney,
}

impl DataSource {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.map(str::trim).unwrap_or_default() {
            "" | "auto" => Ok(Self::Auto),
            "futu" => Ok(Self::Futu),
            "tencent" => Ok(Self::Tencent),
            "eastmoney" => Ok(Self::Eastmoney),
            other => Err(format!(
                "不支持的数据源: {other}（可选 auto、futu、tencent、eastmoney）"
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动",
            Self::Futu => "富途牛牛",
            Self::Tencent => "腾讯行情",
            Self::Eastmoney => "东方财富备用通道",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub tencent_symbols: Vec<String>,
    pub eastmoney_secids: Vec<String>,
    pub futu_symbols: Vec<String>,
    pub market: Market,
}

#[derive(Debug, Clone)]
pub struct KlineBar {
    pub time_ms: i64,
    pub label: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub amount: f64,
}

#[derive(Debug, Clone)]
pub struct KlineData {
    pub name: String,
    pub bars: Vec<KlineBar>,
}

#[derive(Debug, Clone)]
pub struct MarketData {
    pub name: String,
    pub bars: Vec<KlineBar>,
    pub quote: Option<Quote>,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeaderStock {
    pub code: String,
    pub symbol: String,
    pub name: String,
    pub price: Option<f64>,
    pub change_pct: Option<f64>,
    pub boards: Option<i64>,
    pub first_limit_time: Option<String>,
    pub last_limit_time: Option<String>,
    pub seal_amount: Option<f64>,
    pub break_count: Option<i64>,
    pub turnover_rate: Option<f64>,
    pub amount: Option<f64>,
    pub float_market_cap: Option<f64>,
    pub total_market_cap: Option<f64>,
    pub industry: Option<String>,
    pub stat_days: Option<i64>,
    pub stat_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndustryStat {
    pub industry: String,
    pub count: usize,
    pub ratio: f64,
    pub max_boards: i64,
    pub total_boards: i64,
    pub seal_amount: f64,
    pub amount: f64,
    pub avg_turnover: f64,
    pub top_stock: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeaderBoard {
    pub date: String,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub pages: usize,
    pub industry: Option<String>,
    pub industries: Vec<IndustryStat>,
    pub items: Vec<LeaderStock>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Quote {
    pub name: String,
    pub code: String,
    pub price: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub open: Option<f64>,
    pub prev_close: Option<f64>,
    pub change: Option<f64>,
    pub change_pct: Option<f64>,
    pub volume: Option<f64>,
    pub amount: Option<f64>,
    pub time: Option<String>,
    pub delayed: bool,
}

pub fn parse_symbol(input: &str) -> Result<SymbolInfo, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("请输入股票代码".to_string());
    }
    let normalized = raw.to_ascii_lowercase();

    if let Some(code) = normalized.strip_prefix("sh") {
        let code = parse_a_share_code(code, raw)?;
        return Ok(SymbolInfo {
            tencent_symbols: vec![format!("sh{code}")],
            eastmoney_secids: vec![format!("1.{code}")],
            futu_symbols: vec![format!("SH.{code}")],
            market: Market::AShare,
        });
    }
    if let Some(code) = normalized.strip_prefix("sz") {
        let code = parse_a_share_code(code, raw)?;
        return Ok(SymbolInfo {
            tencent_symbols: vec![format!("sz{code}")],
            eastmoney_secids: vec![format!("0.{code}")],
            futu_symbols: vec![format!("SZ.{code}")],
            market: Market::AShare,
        });
    }
    if let Some(code) = normalized.strip_prefix("bj") {
        let code = parse_a_share_code(code, raw)?;
        return Ok(SymbolInfo {
            tencent_symbols: vec![format!("bj{code}")],
            eastmoney_secids: vec![format!("0.{code}")],
            futu_symbols: vec![format!("BJ.{code}")],
            market: Market::AShare,
        });
    }
    if let Some(code) = normalized.strip_prefix("hk") {
        let code = parse_hk_code(code, raw)?;
        return Ok(SymbolInfo {
            tencent_symbols: vec![format!("hk{code}")],
            eastmoney_secids: vec![format!("116.{code}")],
            futu_symbols: vec![format!("HK.{code}")],
            market: Market::HongKong,
        });
    }
    if let Some(ticker) = normalized.strip_prefix("us") {
        return parse_us_symbol(ticker, raw);
    }
    if normalized.len() == 6 && normalized.chars().all(|c| c.is_ascii_digit()) {
        let (prefix, eastmoney_market) = match normalized.as_bytes()[0] {
            b'9' if normalized.starts_with("920") => ("bj", 0),
            b'6' | b'9' | b'5' => ("sh", 1u16),
            b'0' | b'1' | b'2' | b'3' => ("sz", 0),
            b'4' | b'8' => ("bj", 0),
            _ => return Err(format!("无法判断 {raw} 所属市场，请添加 sh/sz/bj 前缀")),
        };
        return Ok(SymbolInfo {
            tencent_symbols: vec![format!("{prefix}{normalized}")],
            eastmoney_secids: vec![format!("{eastmoney_market}.{normalized}")],
            futu_symbols: vec![format!("{}.{normalized}", prefix.to_ascii_uppercase())],
            market: Market::AShare,
        });
    }

    Err(format!(
        "无法识别的股票代码格式: {raw}（示例：sh600000、000001、hk00700、usAAPL）"
    ))
}

fn parse_a_share_code(code: &str, raw: &str) -> Result<String, String> {
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("A股代码应为6位数字: {raw}"));
    }
    Ok(code.to_string())
}

fn parse_hk_code(code: &str, raw: &str) -> Result<String, String> {
    if code.is_empty() || code.len() > 5 || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("港股代码应为1-5位数字: {raw}"));
    }
    Ok(format!("{code:0>5}"))
}

fn parse_us_symbol(input: &str, raw: &str) -> Result<SymbolInfo, String> {
    let (base, suffix) = match input.split_once('.') {
        Some((base, suffix)) => (base, Some(suffix.to_ascii_uppercase())),
        None => (input, None),
    };

    if base.is_empty()
        || base.len() > 12
        || !base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("美股代码格式不正确: {raw}"));
    }
    if let Some(suffix) = &suffix
        && (suffix.is_empty()
            || suffix.len() > 3
            || !suffix.chars().all(|c| c.is_ascii_alphabetic()))
    {
        return Err(format!("美股市场后缀格式不正确: {raw}"));
    }

    let base = base.to_ascii_uppercase();
    let (tencent_symbols, eastmoney_secids) = match suffix {
        Some(suffix) => {
            let eastmoney_market = match suffix.as_str() {
                "N" => 106,
                "A" => 107,
                _ => 105,
            };
            (
                vec![format!("us{base}.{suffix}")],
                vec![format!("{eastmoney_market}.{base}")],
            )
        }
        None => (
            vec![
                format!("us{base}.OQ"),
                format!("us{base}.N"),
                format!("us{base}.A"),
            ],
            vec![
                format!("105.{base}"),
                format!("106.{base}"),
                format!("107.{base}"),
            ],
        ),
    };

    Ok(SymbolInfo {
        tencent_symbols,
        eastmoney_secids,
        futu_symbols: vec![format!("US.{base}")],
        market: Market::Us,
    })
}

pub fn parse_time_ms(value: &str) -> Option<i64> {
    let value = value.trim();
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(datetime) = NaiveDateTime::parse_from_str(value, format) {
            return Some(datetime.and_utc().timestamp_millis());
        }
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|datetime| datetime.and_utc().timestamp_millis())
}

fn parse_tencent_time(raw: &str) -> Option<(i64, String)> {
    if let Ok(datetime) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Some((datetime.and_utc().timestamp_millis(), raw.to_string()));
    }
    if let Ok(datetime) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M") {
        return Some((datetime.and_utc().timestamp_millis(), raw.to_string()));
    }
    if let Ok(datetime) = NaiveDateTime::parse_from_str(raw, "%Y%m%d%H%M%S") {
        return Some((
            datetime.and_utc().timestamp_millis(),
            datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
        ));
    }
    if let Ok(datetime) = NaiveDateTime::parse_from_str(raw, "%Y%m%d%H%M") {
        return Some((
            datetime.and_utc().timestamp_millis(),
            datetime.format("%Y-%m-%d %H:%M").to_string(),
        ));
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some((
            date.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis(),
            raw.to_string(),
        ));
    }
    None
}

pub async fn fetch_market_data(
    http: &reqwest::Client,
    symbol: &SymbolInfo,
    period: Period,
    limit: usize,
    adjust: u8,
    source: DataSource,
) -> Result<MarketData, String> {
    let mut errors = Vec::new();
    let order: &[DataSource] = match source {
        DataSource::Auto => &[DataSource::Futu, DataSource::Tencent, DataSource::Eastmoney],
        DataSource::Futu => &[DataSource::Futu],
        DataSource::Tencent => &[DataSource::Tencent],
        DataSource::Eastmoney => &[DataSource::Eastmoney],
    };

    for &provider in order {
        if provider == DataSource::Futu && !futu_configured() {
            if source == DataSource::Auto {
                continue;
            }
            errors.push(format!(
                "{}: 未配置 FUTU_ACCESS_TOKEN 或 FUTU_TOKEN",
                provider.label()
            ));
            continue;
        }

        let result = match provider {
            DataSource::Futu => fetch_futu_market_data(http, symbol, period, limit, adjust).await,
            DataSource::Tencent => {
                fetch_tencent_market_data(http, symbol, period, limit, adjust).await
            }
            DataSource::Eastmoney => {
                fetch_eastmoney_market_data(http, symbol, period, limit, adjust).await
            }
            DataSource::Auto => unreachable!(),
        };

        match result {
            Ok(data) => return Ok(data),
            Err(error) => errors.push(format!("{}: {error}", provider.label())),
        }
    }

    Err(format!("行情获取失败: {}", errors.join("；")))
}

pub async fn fetch_quote(
    http: &reqwest::Client,
    symbol: &SymbolInfo,
    source: DataSource,
) -> Result<Quote, String> {
    let data = fetch_market_data(http, symbol, Period::Day, 3, 0, source).await?;
    data.quote.ok_or_else(|| "实时报价暂不可用".to_string())
}

pub async fn fetch_daily_leaders(
    http: &reqwest::Client,
    page: usize,
    page_size: usize,
    industry: Option<&str>,
) -> Result<LeaderBoard, String> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 100);
    let today = Utc::now().with_timezone(&chrono_tz::Asia::Shanghai);
    let industry = industry.map(str::trim).filter(|value| !value.is_empty());

    let mut empty_board: Option<LeaderBoard> = None;
    for offset in 0..=7 {
        let date = today - ChronoDuration::days(offset);
        let date_text = date.format("%Y%m%d").to_string();
        let all = fetch_all_leaders_for_date(http, &date_text).await?;
        if all.is_empty() {
            if empty_board.is_none() {
                empty_board = Some(LeaderBoard {
                    date: format_leader_date(&date_text),
                    total: 0,
                    page,
                    page_size,
                    pages: 0,
                    industry: industry.map(str::to_string),
                    industries: Vec::new(),
                    items: Vec::new(),
                });
            }
            continue;
        }

        let industries = build_industry_stats(&all);
        let filtered: Vec<LeaderStock> = match industry {
            Some(selected) => all
                .into_iter()
                .filter(|item| item.industry.as_deref() == Some(selected))
                .collect(),
            None => all,
        };
        let total = filtered.len();
        let pages = total.div_ceil(page_size);
        let start = (page - 1).saturating_mul(page_size);
        let items = if start >= total {
            Vec::new()
        } else {
            filtered.into_iter().skip(start).take(page_size).collect()
        };

        return Ok(LeaderBoard {
            date: format_leader_date(&date_text),
            total,
            page,
            page_size,
            pages,
            industry: industry.map(str::to_string),
            industries,
            items,
        });
    }

    Ok(empty_board.expect("至少尝试过一个日期"))
}

async fn fetch_all_leaders_for_date(
    http: &reqwest::Client,
    date: &str,
) -> Result<Vec<LeaderStock>, String> {
    let url = format!(
        "https://push2ex.eastmoney.com/getTopicZTPool?ut=7eea3edcaed734bea9cbfc24409ed989&dpt=wz.ztzt&Pageindex=0&pagesize=500&sort=lbc%3Adesc&date={date}"
    );
    let bytes = fetch_url(http, &url).await?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("解析龙头股数据失败: {error}"))?;
    parse_leader_items(&value)
}

fn parse_leader_items(value: &Value) -> Result<Vec<LeaderStock>, String> {
    let rc = value.get("rc").and_then(Value::as_i64).unwrap_or(-1);
    if rc != 0 {
        return Err(format!("东方财富涨停板接口提示 rc={rc}"));
    }

    Ok(value
        .get("data")
        .filter(|data| !data.is_null())
        .and_then(|data| data.get("pool"))
        .and_then(Value::as_array)
        .map(|pool| pool.iter().filter_map(parse_leader_stock).collect())
        .unwrap_or_default())
}

fn build_industry_stats(items: &[LeaderStock]) -> Vec<IndustryStat> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, Vec<&LeaderStock>> = BTreeMap::new();
    for item in items {
        let name = item
            .industry
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "其他".to_string());
        groups.entry(name).or_default().push(item);
    }

    let total = items.len().max(1) as f64;
    let mut stats: Vec<IndustryStat> = groups
        .into_iter()
        .map(|(industry, group)| {
            let turnover: Vec<f64> = group.iter().filter_map(|item| item.turnover_rate).collect();
            let avg_turnover = if turnover.is_empty() {
                0.0
            } else {
                turnover.iter().sum::<f64>() / turnover.len() as f64
            };
            let top_stock = group
                .iter()
                .max_by_key(|item| item.boards.unwrap_or(0))
                .map(|item| item.name.clone())
                .unwrap_or_default();

            IndustryStat {
                industry,
                count: group.len(),
                ratio: group.len() as f64 / total * 100.0,
                max_boards: group
                    .iter()
                    .filter_map(|item| item.boards)
                    .max()
                    .unwrap_or(0),
                total_boards: group.iter().filter_map(|item| item.boards).sum(),
                seal_amount: group.iter().filter_map(|item| item.seal_amount).sum(),
                amount: group.iter().filter_map(|item| item.amount).sum(),
                avg_turnover,
                top_stock,
            }
        })
        .collect();

    stats.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| right.max_boards.cmp(&left.max_boards))
            .then_with(|| left.industry.cmp(&right.industry))
    });
    stats
}

fn parse_leader_stock(item: &Value) -> Option<LeaderStock> {
    let code = item.get("c").and_then(value_as_string)?;
    let market = item.get("m").and_then(Value::as_i64).unwrap_or(0);
    let statistics = item.get("zttj");

    Some(LeaderStock {
        symbol: leader_symbol(&code, market),
        code,
        name: item
            .get("n")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        price: item
            .get("p")
            .and_then(value_as_f64)
            .map(|value| value / 1000.0),
        change_pct: item.get("zdp").and_then(value_as_f64),
        boards: item.get("lbc").and_then(Value::as_i64),
        first_limit_time: item
            .get("fbt")
            .and_then(Value::as_i64)
            .and_then(format_board_time),
        last_limit_time: item
            .get("lbt")
            .and_then(Value::as_i64)
            .and_then(format_board_time),
        seal_amount: item.get("fund").and_then(value_as_f64),
        break_count: item.get("zbc").and_then(Value::as_i64),
        turnover_rate: item.get("hs").and_then(value_as_f64),
        amount: item.get("amount").and_then(value_as_f64),
        float_market_cap: item.get("ltsz").and_then(value_as_f64),
        total_market_cap: item.get("tshare").and_then(value_as_f64),
        industry: item
            .get("hybk")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
        stat_days: statistics
            .and_then(|value| value.get("days"))
            .and_then(Value::as_i64),
        stat_count: statistics
            .and_then(|value| value.get("ct"))
            .and_then(Value::as_i64),
    })
}

fn leader_symbol(code: &str, market: i64) -> String {
    if market == 1 {
        format!("sh{code}")
    } else if code.starts_with('4') || code.starts_with('8') || code.starts_with("920") {
        format!("bj{code}")
    } else {
        format!("sz{code}")
    }
}

fn format_board_time(value: i64) -> Option<String> {
    if value <= 0 || value > 235_959 {
        return None;
    }
    let text = format!("{value:06}");
    Some(format!("{}:{}:{}", &text[0..2], &text[2..4], &text[4..6]))
}

fn format_leader_date(value: &str) -> String {
    if value.len() == 8 && value.chars().all(|character| character.is_ascii_digit()) {
        format!("{}-{}-{}", &value[0..4], &value[4..6], &value[6..8])
    } else {
        value.to_string()
    }
}

pub fn futu_configured() -> bool {
    futu_token().is_some()
}

fn futu_token() -> Option<String> {
    ["FUTU_ACCESS_TOKEN", "FUTU_TOKEN"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn futu_base_url() -> String {
    std::env::var("FUTU_BASE_URL")
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| "https://webapi.futunn.com".to_string())
}

async fn fetch_tencent_market_data(
    http: &reqwest::Client,
    symbol: &SymbolInfo,
    period: Period,
    limit: usize,
    adjust: u8,
) -> Result<MarketData, String> {
    let mut last_error = String::new();

    if matches!(symbol.market, Market::HongKong | Market::Us) && period.is_intraday() {
        last_error = "腾讯接口暂不支持港股/美股分钟K线".to_string();
    } else {
        for candidate in &symbol.tencent_symbols {
            match fetch_tencent(http, candidate, symbol.market, period, limit, adjust).await {
                Ok(mut data) => {
                    align_bar_times(&mut data.bars, symbol.market, period, data.quote.as_ref());
                    return Ok(data);
                }
                Err(error) => last_error = error,
            }
        }
    }

    Err(last_error)
}

async fn fetch_eastmoney_market_data(
    http: &reqwest::Client,
    symbol: &SymbolInfo,
    period: Period,
    limit: usize,
    adjust: u8,
) -> Result<MarketData, String> {
    let mut last_error = String::new();

    for secid in &symbol.eastmoney_secids {
        let (kline_result, quote_result) = tokio::join!(
            fetch_eastmoney_kline(http, secid, period, limit, adjust),
            fetch_eastmoney_quote(http, secid),
        );
        match kline_result {
            Ok(mut kline) => {
                let quote = quote_result.ok();
                align_bar_times(&mut kline.bars, symbol.market, period, quote.as_ref());
                return Ok(MarketData {
                    name: kline.name,
                    bars: kline.bars,
                    quote,
                    source: "eastmoney",
                });
            }
            Err(error) => last_error = error,
        }
    }

    Err(last_error)
}

async fn fetch_futu_market_data(
    http: &reqwest::Client,
    symbol: &SymbolInfo,
    period: Period,
    limit: usize,
    adjust: u8,
) -> Result<MarketData, String> {
    let token = futu_token().ok_or_else(|| {
        "未配置 FUTU_ACCESS_TOKEN 或 FUTU_TOKEN（富途 OAuth access_token）".to_string()
    })?;
    let base = futu_base_url();
    let mut last_error = String::new();

    for code in &symbol.futu_symbols {
        let (kline_result, quote_result) = tokio::join!(
            fetch_futu_kline(
                http,
                &base,
                &token,
                code,
                symbol.market,
                period,
                limit,
                adjust
            ),
            fetch_futu_quote(http, &base, &token, code, symbol.market),
        );
        match kline_result {
            Ok(mut kline) => {
                let quote = quote_result.ok();
                align_bar_times(&mut kline.bars, symbol.market, period, quote.as_ref());
                let name = quote
                    .as_ref()
                    .map(|quote| quote.name.clone())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or(kline.name);
                return Ok(MarketData {
                    name,
                    bars: kline.bars,
                    quote,
                    source: "futu",
                });
            }
            Err(error) => last_error = error,
        }
    }

    Err(if last_error.is_empty() {
        "富途未返回该代码的数据".to_string()
    } else {
        last_error
    })
}

struct FutuKlinePage {
    bars: Vec<KlineBar>,
    next_time: i64,
    next_page_token: Option<String>,
    has_more: bool,
    name: String,
}

#[allow(clippy::too_many_arguments)]
async fn fetch_futu_kline(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    code: &str,
    market: Market,
    period: Period,
    limit: usize,
    adjust: u8,
) -> Result<KlineData, String> {
    let page_size = if limit > 370 { 370 } else { limit.max(1) };
    let end = (Utc::now() + ChronoDuration::days(2))
        .format("%Y-%m-%d")
        .to_string();
    let mut all_bars = Vec::new();
    let mut next_time = 0i64;
    let mut next_page_token: Option<String> = None;
    let mut first = true;
    let mut name = String::new();

    for _ in 0..8 {
        if all_bars.len() >= limit {
            break;
        }
        let num = if next_page_token.is_some() || limit > 370 {
            page_size
        } else {
            (limit - all_bars.len()).min(page_size)
        };
        let page = fetch_futu_kline_page(
            http,
            base,
            token,
            code,
            market,
            period,
            num,
            adjust,
            first,
            &end,
            next_time,
            next_page_token.as_deref(),
        )
        .await?;

        if name.trim().is_empty() {
            name = page.name.clone();
        }
        if page.bars.is_empty() {
            break;
        }
        all_bars.extend(page.bars);
        if !page.has_more {
            break;
        }
        next_page_token = page.next_page_token;
        next_time = page.next_time;
        first = false;
        if next_page_token.is_none() && next_time <= 0 {
            break;
        }
    }

    let bars = clean_bars(all_bars, limit);
    if bars.is_empty() {
        return Err("富途未返回有效的K线数据".to_string());
    }
    Ok(KlineData {
        name: if name.trim().is_empty() {
            "未知名称".to_string()
        } else {
            name
        },
        bars,
    })
}

#[allow(clippy::too_many_arguments)]
async fn fetch_futu_kline_page(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    code: &str,
    market: Market,
    period: Period,
    num: usize,
    adjust: u8,
    first: bool,
    end: &str,
    next_time: i64,
    page_token: Option<&str>,
) -> Result<FutuKlinePage, String> {
    let mut url = format!(
        "{base}/api/v1.0/quote/{code}/history-kline?num={num}&ktype={}&autype={}&extended_time=0",
        futu_ktype(period),
        futu_autype(market, period, adjust),
    );
    if first {
        url.push_str(&format!("&end={end}"));
    } else if let Some(page_token_value) = page_token {
        url.push_str(&format!("&page_token={page_token_value}"));
    } else if next_time > 0 {
        url.push_str(&format!("&next_time={next_time}"));
    }

    let value = fetch_futu_json(http, &url, token, None).await?;
    let data = value
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| "富途接口未返回K线数据".to_string())?;
    let list = data
        .get("kline_list")
        .and_then(Value::as_array)
        .ok_or_else(|| "富途接口未返回K线列表".to_string())?;
    let bars = parse_futu_bars(list, market, period);
    let next_time = data.get("next_time").and_then(Value::as_i64).unwrap_or(0);
    let pagination = data.get("pagination");
    let has_more = pagination
        .and_then(|page| page.get("has_more"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let next_page_token = pagination
        .and_then(|page| page.get("next_page_token"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let name = list
        .iter()
        .find_map(|item| {
            let text = item
                .get("sc_name")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
                .or_else(|| item.get("tc_name").and_then(Value::as_str))?;
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        })
        .unwrap_or_default();

    Ok(FutuKlinePage {
        bars,
        next_time,
        next_page_token,
        has_more,
        name,
    })
}

async fn fetch_futu_quote(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    code: &str,
    market: Market,
) -> Result<Quote, String> {
    let url = format!("{base}/api/v1.0/quote/stock-quote");
    let body = serde_json::json!({ "code_list": [code] });
    let value = fetch_futu_json(http, &url, token, Some(&body)).await?;
    let quote = value
        .get("data")
        .and_then(|data| data.get("quote_list"))
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .ok_or_else(|| "富途接口未返回实时报价".to_string())?;

    let price = quote
        .get("last_price")
        .and_then(value_as_f64)
        .ok_or_else(|| "富途实时报价缺少最新价".to_string())?;
    let prev_close = quote.get("prev_close_price").and_then(value_as_f64);
    let change = prev_close.map(|prev| price - prev);
    let change_pct = prev_close
        .and_then(|prev| (prev.abs() > f64::EPSILON).then(|| (price - prev) / prev * 100.0));
    let time = quote
        .get("data_time")
        .and_then(Value::as_i64)
        .and_then(|ms| format_futu_time(ms, market, "%Y-%m-%d %H:%M:%S"));
    let name = quote
        .get("sc_name")
        .and_then(Value::as_str)
        .or_else(|| quote.get("name").and_then(Value::as_str))
        .or_else(|| quote.get("tc_name").and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_string();

    Ok(Quote {
        name: if name.is_empty() {
            "未知名称".to_string()
        } else {
            name
        },
        code: quote
            .get("code")
            .and_then(value_as_string)
            .unwrap_or_else(|| code.to_string()),
        price: Some(price),
        high: quote.get("high_price").and_then(value_as_f64),
        low: quote.get("low_price").and_then(value_as_f64),
        open: quote.get("open_price").and_then(value_as_f64),
        prev_close,
        change,
        change_pct,
        volume: quote.get("volume").and_then(value_as_f64),
        amount: quote.get("turnover").and_then(value_as_f64),
        time,
        delayed: false,
    })
}

async fn fetch_futu_json(
    http: &reqwest::Client,
    url: &str,
    token: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let builder = if let Some(body) = body {
        http.post(url).json(body)
    } else {
        http.get(url)
    };
    let response = builder
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("请求富途接口失败: {error}"))?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取富途响应失败: {error}"))?;
    let parsed = serde_json::from_slice::<Value>(&bytes);

    if !status.is_success() {
        let message = parsed
            .as_ref()
            .ok()
            .and_then(futu_error_message)
            .unwrap_or("无错误详情");
        return Err(format!("富途接口返回 HTTP {status}: {message}"));
    }

    let value = parsed.map_err(|error| format!("解析富途响应失败: {error}"))?;
    let ret_code = value.get("ret_code").and_then(Value::as_i64).unwrap_or(-1);
    if ret_code != 0 {
        let message = futu_error_message(&value).unwrap_or("未知错误");
        return Err(format!("富途接口提示: {message}（ret_code={ret_code}）"));
    }
    Ok(value)
}

fn futu_error_message(value: &Value) -> Option<&str> {
    value.get("ret_msg").and_then(Value::as_str).or_else(|| {
        value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
    })
}

fn futu_ktype(period: Period) -> u8 {
    match period {
        Period::Min1 => 1,
        Period::Min5 => 6,
        Period::Min15 => 7,
        Period::Min30 => 8,
        Period::Min60 => 9,
        Period::Day => 2,
        Period::Week => 3,
        Period::Month => 4,
    }
}

fn futu_autype(market: Market, period: Period, adjust: u8) -> u8 {
    if market == Market::AShare && !period.is_intraday() {
        match adjust {
            1 => 1,
            2 => 2,
            _ => 0,
        }
    } else {
        0
    }
}

fn futu_market_timezone(market: Market) -> Tz {
    match market {
        Market::AShare => chrono_tz::Asia::Shanghai,
        Market::HongKong => chrono_tz::Asia::Hong_Kong,
        Market::Us => chrono_tz::America::New_York,
    }
}

fn format_futu_time(time_ms: i64, market: Market, format: &str) -> Option<String> {
    let datetime = chrono::DateTime::from_timestamp_millis(time_ms)?;
    Some(
        datetime
            .with_timezone(&futu_market_timezone(market))
            .format(format)
            .to_string(),
    )
}

fn market_midnight_ms(date: NaiveDate, market: Market) -> Option<i64> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    futu_market_timezone(market)
        .from_local_datetime(&naive)
        .earliest()
        .map(|datetime| datetime.timestamp_millis())
}

fn parse_futu_date_value(value: Option<&Value>, market: Market) -> Option<NaiveDate> {
    match value? {
        Value::Number(number) => {
            let number = number.as_i64()?;
            if (19_000_101..=21_001_231).contains(&number) {
                NaiveDate::from_ymd_opt(
                    (number / 10_000) as i32,
                    ((number / 100) % 100) as u32,
                    (number % 100) as u32,
                )
            } else {
                chrono::DateTime::from_timestamp_millis(number).map(|datetime| {
                    datetime
                        .with_timezone(&futu_market_timezone(market))
                        .date_naive()
                })
            }
        }
        Value::String(text) => {
            for format in ["%Y%m%d", "%Y-%m-%d"] {
                if let Ok(date) = NaiveDate::parse_from_str(text.trim(), format) {
                    return Some(date);
                }
            }
            None
        }
        _ => None,
    }
}

fn futu_date_label(item: &Value, time_ms: i64, market: Market) -> Option<String> {
    parse_futu_date_value(item.get("date"), market)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .or_else(|| format_futu_time(time_ms, market, "%Y-%m-%d"))
}

fn parse_futu_bars(list: &[Value], market: Market, period: Period) -> Vec<KlineBar> {
    let mut bars = Vec::with_capacity(list.len());

    for item in list {
        let date_value = item.get("date");
        let time_ms = item
            .get("time_key")
            .and_then(Value::as_i64)
            .or_else(|| {
                date_value
                    .and_then(Value::as_i64)
                    .filter(|number| *number > 1_000_000_000_000)
            })
            .or_else(|| {
                parse_futu_date_value(date_value, market)
                    .and_then(|date| market_midnight_ms(date, market))
            });
        let Some(time_ms) = time_ms else {
            continue;
        };
        let (Some(open), Some(close), Some(high), Some(low), Some(volume)) = (
            item.get("open")
                .or_else(|| item.get("open_price"))
                .and_then(value_as_f64),
            item.get("close")
                .or_else(|| item.get("close_price"))
                .and_then(value_as_f64),
            item.get("high")
                .or_else(|| item.get("high_price"))
                .and_then(value_as_f64),
            item.get("low")
                .or_else(|| item.get("low_price"))
                .and_then(value_as_f64),
            item.get("volume").and_then(value_as_f64),
        ) else {
            continue;
        };
        let amount = item.get("turnover").and_then(value_as_f64).unwrap_or(0.0);
        let label = if period.is_intraday() {
            format_futu_time(time_ms, market, "%Y-%m-%d %H:%M")
                .or_else(|| futu_date_label(item, time_ms, market))
                .unwrap_or_default()
        } else {
            futu_date_label(item, time_ms, market).unwrap_or_default()
        };

        bars.push(KlineBar {
            time_ms,
            label,
            open,
            high,
            low,
            close,
            volume,
            amount,
        });
    }

    bars
}

async fn fetch_tencent(
    http: &reqwest::Client,
    symbol: &str,
    market: Market,
    period: Period,
    limit: usize,
    adjust: u8,
) -> Result<MarketData, String> {
    let (url, key) = if period.is_intraday() {
        if !matches!(market, Market::AShare) {
            return Err("腾讯接口暂不支持港股/美股分钟K线".to_string());
        }
        let minutes = period.klt();
        (
            format!(
                "https://ifzq.gtimg.cn/appstock/app/kline/mkline?param={symbol},m{minutes},,{limit}"
            ),
            format!("m{minutes}"),
        )
    } else {
        let period_key = match period {
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
            _ => unreachable!(),
        };
        let fq = if market == Market::AShare {
            match adjust {
                1 => "qfq",
                2 => "hfq",
                _ => "",
            }
        } else {
            ""
        };
        let key = if market == Market::AShare {
            match adjust {
                1 => format!("qfq{period_key}"),
                2 => format!("hfq{period_key}"),
                _ => period_key.to_string(),
            }
        } else {
            period_key.to_string()
        };
        (
            format!(
                "https://web.ifzq.gtimg.cn/appstock/app/fqkline/get?param={symbol},{period_key},,,{limit},{fq}"
            ),
            key,
        )
    };

    let bytes = fetch_url(http, &url).await?;
    let text = std::str::from_utf8(&bytes).map_err(|error| format!("解析腾讯行情失败: {error}"))?;
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("解析腾讯行情失败: {error}"))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let message = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("未知错误");
        return Err(format!("腾讯行情接口提示: {message}"));
    }

    let data = value
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| "腾讯行情接口未返回数据".to_string())?;
    let node = data
        .get(symbol)
        .or_else(|| data.values().next())
        .and_then(Value::as_object)
        .ok_or_else(|| "腾讯行情接口未返回该代码的数据".to_string())?;

    let array = node
        .get(&key)
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .or_else(|| {
            node.iter().find_map(|(name, value)| {
                if matches!(
                    name.as_str(),
                    "qt" | "mx_price" | "prec" | "version" | "pandata" | "vcm"
                ) {
                    return None;
                }
                value.as_array().filter(|items| !items.is_empty())
            })
        })
        .ok_or_else(|| "该代码暂无历史K线数据".to_string())?;

    let bars = clean_bars(parse_tencent_bars(array), limit);
    if bars.is_empty() {
        return Err("该代码暂无有效历史K线数据".to_string());
    }

    let quote = parse_tencent_quote(node, symbol, market);
    let name = quote
        .as_ref()
        .map(|quote| quote.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "未知名称".to_string());

    Ok(MarketData {
        name,
        bars,
        quote,
        source: "tencent",
    })
}

fn parse_tencent_bars(array: &[Value]) -> Vec<KlineBar> {
    let mut bars = Vec::with_capacity(array.len());

    for item in array {
        let Some(parts) = item.as_array() else {
            continue;
        };
        if parts.len() < 6 {
            continue;
        }
        let Some(raw_time) = parts[0].as_str() else {
            continue;
        };
        let Some((time_ms, label)) = parse_tencent_time(raw_time) else {
            continue;
        };
        let (Some(open), Some(close), Some(high), Some(low), Some(volume)) = (
            value_as_f64(&parts[1]),
            value_as_f64(&parts[2]),
            value_as_f64(&parts[3]),
            value_as_f64(&parts[4]),
            value_as_f64(&parts[5]),
        ) else {
            continue;
        };

        bars.push(KlineBar {
            time_ms,
            label,
            open,
            high,
            low,
            close,
            volume,
            amount: 0.0,
        });
    }

    bars
}

fn align_bar_times(bars: &mut [KlineBar], market: Market, period: Period, quote: Option<&Quote>) {
    if period.is_intraday() || bars.is_empty() {
        return;
    }

    let close_minutes = match market {
        Market::AShare => 15 * 60,
        Market::HongKong | Market::Us => 16 * 60,
    };
    let close_offset_ms = close_minutes as i64 * 60_000;
    let bar_date = |time_ms: i64| {
        chrono::DateTime::from_timestamp_millis(time_ms).map(|datetime| datetime.date_naive())
    };
    let last_date = bar_date(bars[bars.len() - 1].time_ms);
    let last_quote_ms = quote
        .and_then(|quote| quote.time.as_deref())
        .and_then(parse_quote_datetime)
        .filter(|datetime| Some(datetime.date()) == last_date)
        .and_then(|datetime| {
            let midnight = datetime
                .date()
                .and_hms_opt(0, 0, 0)?
                .and_utc()
                .timestamp_millis();
            Some(
                datetime
                    .and_utc()
                    .timestamp_millis()
                    .clamp(midnight, midnight + close_offset_ms),
            )
        });

    let last = bars.len() - 1;
    for (index, bar) in bars.iter_mut().enumerate() {
        let midnight = bar_date(bar.time_ms)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|datetime| datetime.and_utc().timestamp_millis())
            .unwrap_or(bar.time_ms);
        let close_ms = midnight + close_offset_ms;
        bar.time_ms = if index == last {
            last_quote_ms.unwrap_or(close_ms)
        } else {
            close_ms
        };
    }
}

fn parse_quote_datetime(value: &str) -> Option<NaiveDateTime> {
    let value = value.trim();
    for format in [
        "%Y%m%d%H%M%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(datetime) = NaiveDateTime::parse_from_str(value, format) {
            return Some(datetime);
        }
    }
    None
}

fn clean_bars(mut bars: Vec<KlineBar>, limit: usize) -> Vec<KlineBar> {
    bars.retain(|bar| {
        let prices = [bar.open, bar.high, bar.low, bar.close];
        prices.iter().all(|value| value.is_finite() && *value > 0.0)
            && bar.high >= bar.open.max(bar.close)
            && bar.low <= bar.open.min(bar.close)
            && bar.high >= bar.low
            && bar.volume.is_finite()
            && bar.volume >= 0.0
    });

    bars.sort_by_key(|bar| bar.time_ms);

    let mut deduped: Vec<KlineBar> = Vec::with_capacity(bars.len());
    for bar in bars {
        match deduped.last_mut() {
            Some(last) if last.time_ms == bar.time_ms => *last = bar,
            _ => deduped.push(bar),
        }
    }

    if deduped.len() > limit {
        deduped.split_off(deduped.len() - limit)
    } else {
        deduped
    }
}

fn parse_tencent_quote(
    node: &serde_json::Map<String, Value>,
    symbol: &str,
    market: Market,
) -> Option<Quote> {
    let qt = node.get("qt")?.as_object()?;
    let quote_array = qt.get(symbol).and_then(Value::as_array).or_else(|| {
        qt.values()
            .find_map(|value| value.as_array().filter(|items| items.len() > 34))
    })?;
    let delayed = quote_array
        .first()
        .and_then(Value::as_str)
        .is_some_and(|status| status.to_ascii_lowercase().contains("delay"));
    let name = quote_array.get(1).and_then(Value::as_str)?.to_string();
    let code = quote_array
        .get(2)
        .and_then(value_as_string)
        .unwrap_or_default();
    let price = quote_array.get(3).and_then(value_as_f64)?;
    let prev_close = quote_array.get(4).and_then(value_as_f64);
    let open = quote_array.get(5).and_then(value_as_f64);
    let volume = quote_array.get(6).and_then(value_as_f64);
    let change = quote_array.get(31).and_then(value_as_f64);
    let change_pct = quote_array.get(32).and_then(value_as_f64);
    let high = quote_array.get(33).and_then(value_as_f64);
    let low = quote_array.get(34).and_then(value_as_f64);
    let mut amount = quote_array.get(37).and_then(value_as_f64);
    if market == Market::AShare {
        amount = amount.map(|value| value * 10_000.0);
    }
    let time = quote_array.get(30).and_then(value_as_string);

    Some(Quote {
        name,
        code,
        price: Some(price),
        high,
        low,
        open,
        prev_close,
        change,
        change_pct,
        volume,
        amount,
        time,
        delayed,
    })
}

async fn fetch_eastmoney_kline(
    http: &reqwest::Client,
    secid: &str,
    period: Period,
    limit: usize,
    adjust: u8,
) -> Result<KlineData, String> {
    let query = format!(
        "secid={secid}&fields1=f1,f2,f3,f4,f5,f6&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61&klt={}&fqt={adjust}&end=20500101&lmt={limit}",
        period.klt()
    );
    let url = format!("https://push2his.eastmoney.com/api/qt/stock/kline/get?{query}");
    let bytes = fetch_url(http, &url).await?;
    let text = decode_eastmoney_text(&bytes);

    let response: EmKlineResponse =
        serde_json::from_str(&text).map_err(|error| format!("解析K线数据失败: {error}"))?;
    let data = response.data.ok_or_else(|| {
        if response.msg.trim().is_empty() {
            "未找到该代码的K线数据，请检查代码格式或市场前缀".to_string()
        } else {
            format!("行情接口提示: {}", response.msg.trim())
        }
    })?;

    let mut bars = Vec::with_capacity(data.klines.len());
    for line in &data.klines {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 7 {
            continue;
        }
        let Some(time_ms) = parse_time_ms(parts[0]) else {
            continue;
        };
        let (Ok(open), Ok(close), Ok(high), Ok(low), Ok(volume)) = (
            parts[1].trim().parse::<f64>(),
            parts[2].trim().parse::<f64>(),
            parts[3].trim().parse::<f64>(),
            parts[4].trim().parse::<f64>(),
            parts[5].trim().parse::<f64>(),
        ) else {
            continue;
        };
        let amount = parts[6].trim().parse::<f64>().unwrap_or(0.0);
        bars.push(KlineBar {
            time_ms,
            label: parts[0].trim().to_string(),
            open,
            high,
            low,
            close,
            volume,
            amount,
        });
    }

    let bars = clean_bars(bars, limit);

    if bars.is_empty() {
        return Err("未获取到有效的K线数据".to_string());
    }

    let name = if data.name.trim().is_empty() {
        "未知名称".to_string()
    } else {
        data.name.trim().to_string()
    };

    Ok(KlineData { name, bars })
}

async fn fetch_eastmoney_quote(http: &reqwest::Client, secid: &str) -> Result<Quote, String> {
    let query = format!("secid={secid}&fields=f43,f44,f45,f46,f47,f48,f57,f58,f59,f60,f169,f170");
    let url = format!("https://push2.eastmoney.com/api/qt/stock/get?{query}");
    let bytes = fetch_url(http, &url).await?;
    let text = decode_eastmoney_text(&bytes);

    let response: EmQuoteResponse =
        serde_json::from_str(&text).map_err(|error| format!("解析实时报价失败: {error}"))?;
    let data = response
        .data
        .ok_or_else(|| "实时报价暂不可用".to_string())?;
    let fields = data.fields;

    let decimal = field_f64(&fields, "f59").unwrap_or(2.0).clamp(0.0, 6.0) as i32;
    let scale = 10f64.powi(decimal);
    let price = field_f64(&fields, "f43").map(|value| value / scale);

    if price.is_none() {
        return Err("实时报价暂不可用".to_string());
    }

    let fallback_code = secid
        .split_once('.')
        .map(|(_, code)| code.to_string())
        .unwrap_or_else(|| secid.to_string());

    Ok(Quote {
        name: field_string(&fields, "f58").unwrap_or_default(),
        code: field_string(&fields, "f57").unwrap_or(fallback_code),
        price,
        high: field_f64(&fields, "f44").map(|value| value / scale),
        low: field_f64(&fields, "f45").map(|value| value / scale),
        open: field_f64(&fields, "f46").map(|value| value / scale),
        prev_close: field_f64(&fields, "f60").map(|value| value / scale),
        change: field_f64(&fields, "f169").map(|value| value / scale),
        change_pct: field_f64(&fields, "f170").map(|value| value / 100.0),
        volume: field_f64(&fields, "f47"),
        amount: field_f64(&fields, "f48"),
        time: None,
        delayed: false,
    })
}

pub fn demo_kline(count: usize) -> KlineData {
    let count = count.max(3);
    let mut rng = Lcg::new(0x9E37_79B9_7F4A_7C15);
    let mut date = NaiveDate::from_ymd_opt(2025, 1, 2).expect("valid demo date");
    let mut prev_close = 48.0;
    let mut bars = Vec::with_capacity(count);

    while bars.len() < count {
        if date.weekday().num_days_from_monday() < 5 {
            let i = bars.len() as f64;
            let trend = (i * 0.07).sin() * 0.42 + (i * 0.015).cos() * 0.18 + 0.02;
            let open = (prev_close + rng.normal() * 0.25).max(1.0);
            let close = (open + trend + rng.normal() * 0.75).max(1.0);
            let high = open.max(close) + rng.unit() * 0.6;
            let low = (open.min(close) - rng.unit() * 0.6).max(0.5);
            let volume = 200_000.0 + rng.unit() * 1_800_000.0;
            let amount = volume * (open + close) / 2.0;
            let time_ms = date
                .and_hms_opt(0, 0, 0)
                .expect("valid demo time")
                .and_utc()
                .timestamp_millis();

            bars.push(KlineBar {
                time_ms,
                label: date.format("%Y-%m-%d").to_string(),
                open,
                high,
                low,
                close,
                volume,
                amount,
            });
            prev_close = close;
        }
        date += ChronoDuration::days(1);
    }

    KlineData {
        name: "模拟行情".to_string(),
        bars,
    }
}

pub fn demo_quote(data: &KlineData) -> Quote {
    let last = data.bars.last().expect("demo data is not empty");
    let prev_close = data
        .bars
        .get(data.bars.len().saturating_sub(2))
        .map(|bar| bar.close)
        .unwrap_or(last.open);
    let change = last.close - prev_close;
    let change_pct = if prev_close.abs() > f64::EPSILON {
        change / prev_close * 100.0
    } else {
        0.0
    };

    Quote {
        name: data.name.clone(),
        code: "DEMO".to_string(),
        price: Some(last.close),
        high: Some(last.high),
        low: Some(last.low),
        open: Some(last.open),
        prev_close: Some(prev_close),
        change: Some(change),
        change_pct: Some(change_pct),
        volume: Some(last.volume),
        amount: Some(last.amount),
        time: Some(last.label.clone()),
        delayed: false,
    }
}

async fn fetch_url(http: &reqwest::Client, https_url: &str) -> Result<Vec<u8>, String> {
    let mut last_error = String::new();

    for attempt in 0..2 {
        match http.get(https_url).send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    match response.bytes().await {
                        Ok(bytes) => return Ok(bytes.to_vec()),
                        Err(error) => last_error = format!("读取响应失败: {error}"),
                    }
                } else {
                    last_error = format!("行情接口返回 HTTP {status}");
                    if status.is_client_error() {
                        return Err(last_error);
                    }
                }
            }
            Err(error) => last_error = format!("网络请求失败: {error}"),
        }
        if attempt == 0 {
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }

    let Some(rest) = https_url.strip_prefix("https://") else {
        return Err(last_error);
    };
    let http_url = format!("http://{rest}");

    match http.get(&http_url).send().await {
        Ok(response) => {
            let status = response.status();
            if !status.is_success() {
                return Err(format!("{last_error}；HTTP 备用通道返回 HTTP {status}"));
            }
            response
                .bytes()
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|error| format!("{last_error}；HTTP 备用通道读取失败: {error}"))
        }
        Err(error) => Err(format!("{last_error}；HTTP 备用通道失败: {error}")),
    }
}

fn decode_eastmoney_text(bytes: &[u8]) -> String {
    let utf8 = std::str::from_utf8(bytes)
        .ok()
        .map(|text| text.trim_start_matches('\u{feff}').to_string());
    let (gbk, _, _) = encoding_rs::GB18030.decode(bytes);
    let gbk = gbk.into_owned();

    match utf8 {
        Some(text)
            if !text.contains('\u{fffd}') && (!contains_cjk(&gbk) || contains_cjk(&text)) =>
        {
            text
        }
        _ => gbk,
    }
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            character as u32,
            0x3400..=0x9FFF | 0xF900..=0xFAFF
        )
    })
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn field_f64(fields: &HashMap<String, Value>, key: &str) -> Option<f64> {
    fields.get(key).and_then(value_as_f64)
}

fn field_string(fields: &HashMap<String, Value>, key: &str) -> Option<String> {
    fields.get(key).and_then(value_as_string)
}

#[derive(Deserialize)]
struct EmKlineResponse {
    data: Option<EmKlineData>,
    #[serde(default)]
    msg: String,
}

#[derive(Deserialize)]
struct EmKlineData {
    #[serde(default)]
    name: String,
    #[serde(default)]
    klines: Vec<String>,
}

#[derive(Deserialize)]
struct EmQuoteResponse {
    data: Option<EmQuoteData>,
}

#[derive(Deserialize)]
struct EmQuoteData {
    #[serde(flatten)]
    fields: HashMap<String, Value>,
}

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / 9_007_199_254_740_992.0
    }

    fn normal(&mut self) -> f64 {
        let u1 = self.unit().max(1e-12);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_share_symbols() {
        let symbol = parse_symbol("sh600000").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["sh600000"]);
        assert_eq!(symbol.eastmoney_secids, vec!["1.600000"]);
        assert_eq!(symbol.market, Market::AShare);

        let symbol = parse_symbol("600000").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["sh600000"]);

        let symbol = parse_symbol("000001").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["sz000001"]);
        assert_eq!(symbol.eastmoney_secids, vec!["0.000001"]);

        let symbol = parse_symbol("bj430047").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["bj430047"]);
        assert_eq!(symbol.eastmoney_secids, vec!["0.430047"]);

        let symbol = parse_symbol("430047").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["bj430047"]);

        let symbol = parse_symbol("920001").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["bj920001"]);
        assert_eq!(symbol.eastmoney_secids, vec!["0.920001"]);
    }

    #[test]
    fn parses_hk_and_us_symbols() {
        let symbol = parse_symbol("hk00700").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["hk00700"]);
        assert_eq!(symbol.eastmoney_secids, vec!["116.00700"]);
        assert_eq!(symbol.market, Market::HongKong);

        let symbol = parse_symbol("hk700").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["hk00700"]);

        let symbol = parse_symbol("usAAPL").unwrap();
        assert_eq!(
            symbol.tencent_symbols,
            vec!["usAAPL.OQ", "usAAPL.N", "usAAPL.A"]
        );
        assert_eq!(
            symbol.eastmoney_secids,
            vec!["105.AAPL", "106.AAPL", "107.AAPL"]
        );
        assert_eq!(symbol.market, Market::Us);

        let symbol = parse_symbol("usBABA.N").unwrap();
        assert_eq!(symbol.tencent_symbols, vec!["usBABA.N"]);
        assert_eq!(symbol.eastmoney_secids, vec!["106.BABA"]);
    }

    #[test]
    fn rejects_invalid_symbols() {
        assert!(parse_symbol("").is_err());
        assert!(parse_symbol("sh123").is_err());
        assert!(parse_symbol("hkabc").is_err());
        assert!(parse_symbol("us").is_err());
        assert!(parse_symbol("AAPL").is_err());
        assert!(parse_symbol("股票").is_err());
    }

    #[test]
    fn parses_periods() {
        assert_eq!(Period::parse("101").unwrap(), Period::Day);
        assert_eq!(Period::parse("day").unwrap(), Period::Day);
        assert_eq!(Period::parse("60").unwrap(), Period::Min60);
        assert_eq!(Period::parse("week").unwrap(), Period::Week);
        assert!(Period::parse("999").is_err());
        assert_eq!(Period::Day.time_unit(), "天");
        assert_eq!(Period::Min5.time_unit(), "分钟");
        assert_eq!(Period::Min5.minutes(), 5);
        assert_eq!(Period::Day.minutes(), 0);
    }

    #[test]
    fn parses_times() {
        assert_eq!(parse_time_ms("2026-11-25"), Some(1_795_564_800_000));
        assert_eq!(parse_time_ms("2026-11-25 09:35"), Some(1_795_599_300_000));
        assert!(parse_time_ms("not-a-date").is_none());

        let (time_ms, label) = parse_tencent_time("202611251325").unwrap();
        assert_eq!(time_ms, 1_795_613_100_000);
        assert_eq!(label, "2026-11-25 13:25");

        let (time_ms, label) = parse_tencent_time("2026-11-25").unwrap();
        assert_eq!(time_ms, 1_795_564_800_000);
        assert_eq!(label, "2026-11-25");
    }

    #[test]
    fn parses_tencent_bars_and_quote() {
        let value = json!({
            "code": 0,
            "data": {
                "sh600000": {
                    "qfqday": [
                        ["2026-11-24", "10.62", "10.69", "10.75", "10.45", "724504.000"],
                        ["2026-11-25", "10.74", "10.67", "10.79", "10.52", "717581"]
                    ],
                    "qt": {
                        "market": ["2026-11-25 17:30:22|SH_close_交易中"],
                        "sh600000": [
                            "1", "浦发银行", "600000", "10.67", "10.69", "10.74", "717581",
                            "428940", "288641", "10.67", "901", "10.66", "554", "10.65",
                            "221", "10.64", "1912", "10.63", "1874", "10.68", "646",
                            "10.69", "5212", "10.70", "540", "10.71", "763", "10.72",
                            "558", "", "20261125161450", "-0.02", "-0.19", "10.79",
                            "10.52", "10.67/717581/761740930", "717581", "76174", "0.22",
                            "6.86"
                        ]
                    }
                }
            }
        });

        let data = value.get("data").unwrap().as_object().unwrap();
        let node = data.get("sh600000").unwrap().as_object().unwrap();
        let array = node.get("qfqday").unwrap().as_array().unwrap();
        let bars = clean_bars(parse_tencent_bars(array), 10);

        assert_eq!(bars.len(), 2);
        assert_eq!(bars[1].label, "2026-11-25");
        assert_eq!(bars[1].open, 10.74);
        assert_eq!(bars[1].close, 10.67);
        assert_eq!(bars[1].high, 10.79);
        assert_eq!(bars[1].low, 10.52);
        assert_eq!(bars[1].volume, 717581.0);

        let quote = parse_tencent_quote(node, "sh600000", Market::AShare).unwrap();
        assert_eq!(quote.name, "浦发银行");
        assert_eq!(quote.code, "600000");
        assert_eq!(quote.price, Some(10.67));
        assert_eq!(quote.change_pct, Some(-0.19));
        assert_eq!(quote.amount, Some(761_740_000.0));
        assert_eq!(quote.time.as_deref(), Some("20261125161450"));
        assert!(!quote.delayed);
    }

    #[test]
    fn marks_delayed_tencent_quotes() {
        let value = json!({
            "qt": {
                "usAAPL.OQ": ["delay", "苹果", "AAPL.OQ", "354.07"]
            }
        });
        let node = value.as_object().unwrap();
        let quote = parse_tencent_quote(node, "usAAPL.OQ", Market::Us).unwrap();
        assert!(quote.delayed);
        assert_eq!(quote.price, Some(354.07));
    }

    #[test]
    fn aligns_daily_bars_to_market_close() {
        let bar = |time_ms: i64| KlineBar {
            time_ms,
            label: String::new(),
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: 0.0,
            amount: 0.0,
        };
        let day1 = 1_795_564_800_000;
        let day2 = 1_795_651_200_000;

        let mut bars = vec![bar(day1), bar(day2)];
        let quote = Quote {
            time: Some("20261126103000".to_string()),
            ..Quote::default()
        };
        align_bar_times(&mut bars, Market::AShare, Period::Day, Some(&quote));
        assert_eq!(bars[0].time_ms, day1 + 15 * 3_600_000);
        assert_eq!(bars[1].time_ms, day2 + (10 * 60 + 30) * 60_000);

        let mut bars = vec![bar(day2)];
        let quote = Quote {
            time: Some("20261126161450".to_string()),
            ..Quote::default()
        };
        align_bar_times(&mut bars, Market::AShare, Period::Day, Some(&quote));
        assert_eq!(bars[0].time_ms, day2 + 15 * 3_600_000);

        let mut bars = vec![bar(day1)];
        let quote = Quote {
            time: Some("2026-11-25 16:00:02".to_string()),
            ..Quote::default()
        };
        align_bar_times(&mut bars, Market::Us, Period::Day, Some(&quote));
        assert_eq!(bars[0].time_ms, day1 + 16 * 3_600_000);

        let minute = 1_795_599_300_000;
        let mut bars = vec![bar(minute)];
        align_bar_times(&mut bars, Market::AShare, Period::Min5, Some(&quote));
        assert_eq!(bars[0].time_ms, minute);
    }

    #[test]
    fn cleans_invalid_and_duplicate_bars() {
        let bar = |time_ms: i64, open: f64, high: f64, low: f64, close: f64| KlineBar {
            time_ms,
            label: time_ms.to_string(),
            open,
            high,
            low,
            close,
            volume: 100.0,
            amount: 0.0,
        };

        let bars = vec![
            bar(2, 10.0, 10.5, 9.5, 10.2),
            bar(1, 9.0, 9.5, 8.5, 9.2),
            bar(1, 9.1, 9.6, 8.6, 9.3),
            bar(3, 9.5, 10.0, 9.0, 9.8),
            bar(4, 0.0, 1.0, 0.0, 0.5),
        ];

        let cleaned = clean_bars(bars, 10);
        assert_eq!(cleaned.len(), 3);
        assert_eq!(cleaned[0].time_ms, 1);
        assert_eq!(cleaned[0].close, 9.3);
        assert_eq!(cleaned[1].time_ms, 2);
        assert_eq!(cleaned[2].time_ms, 3);

        let limited = clean_bars(
            vec![
                bar(1, 1.0, 1.2, 0.8, 1.0),
                bar(2, 2.0, 2.2, 1.8, 2.0),
                bar(3, 3.0, 3.2, 2.8, 3.0),
            ],
            2,
        );
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].time_ms, 2);
        assert_eq!(limited[1].time_ms, 3);
    }

    #[test]
    fn demo_data_is_consistent() {
        let data = demo_kline(80);
        assert_eq!(data.bars.len(), 80);
        for window in data.bars.windows(2) {
            assert!(window[1].time_ms > window[0].time_ms);
        }
        for bar in &data.bars {
            assert!(bar.low <= bar.open.min(bar.close));
            assert!(bar.high >= bar.open.max(bar.close));
            assert!(bar.volume > 0.0);
        }
    }

    #[test]
    fn demo_quote_matches_last_bar() {
        let data = demo_kline(10);
        let quote = demo_quote(&data);
        let last = data.bars.last().unwrap();
        assert_eq!(quote.price, Some(last.close));
        assert_eq!(quote.code, "DEMO");
        assert_eq!(quote.time.as_deref(), Some(last.label.as_str()));
    }

    #[test]
    fn parses_data_sources() {
        assert_eq!(DataSource::parse(None).unwrap(), DataSource::Auto);
        assert_eq!(DataSource::parse(Some(" auto ")).unwrap(), DataSource::Auto);
        assert_eq!(DataSource::parse(Some("futu")).unwrap(), DataSource::Futu);
        assert_eq!(
            DataSource::parse(Some("tencent")).unwrap(),
            DataSource::Tencent
        );
        assert_eq!(
            DataSource::parse(Some("eastmoney")).unwrap(),
            DataSource::Eastmoney
        );
        assert!(DataSource::parse(Some("sina")).is_err());
    }

    #[test]
    fn parses_futu_symbols() {
        assert_eq!(
            parse_symbol("sh600000").unwrap().futu_symbols,
            vec!["SH.600000"]
        );
        assert_eq!(
            parse_symbol("000001").unwrap().futu_symbols,
            vec!["SZ.000001"]
        );
        assert_eq!(
            parse_symbol("920001").unwrap().futu_symbols,
            vec!["BJ.920001"]
        );
        assert_eq!(
            parse_symbol("hk700").unwrap().futu_symbols,
            vec!["HK.00700"]
        );
        assert_eq!(
            parse_symbol("usAAPL").unwrap().futu_symbols,
            vec!["US.AAPL"]
        );
        assert_eq!(
            parse_symbol("usBABA.N").unwrap().futu_symbols,
            vec!["US.BABA"]
        );
    }

    #[test]
    fn parses_futu_bars_and_periods() {
        let list = vec![
            json!({
                "time_key": 1_795_564_800_000_i64,
                "date": 20261125,
                "open": 10.0,
                "close": 10.5,
                "high": 10.8,
                "low": 9.9,
                "volume": 100,
                "turnover": 1000.0,
                "name": "测试",
            }),
            json!({
                "time_key": 1_795_651_200_000_i64,
                "date": 20261126,
                "open_price": 10.5,
                "close_price": 10.6,
                "high_price": 10.9,
                "low_price": 10.4,
                "volume": 200,
                "turnover": 2000.0,
            }),
        ];

        let bars = parse_futu_bars(&list, Market::AShare, Period::Day);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].label, "2026-11-25");
        assert_eq!(bars[0].open, 10.0);
        assert_eq!(bars[1].label, "2026-11-26");
        assert_eq!(bars[1].close, 10.6);
        assert_eq!(bars[1].amount, 2000.0);

        assert_eq!(futu_ktype(Period::Min5), 6);
        assert_eq!(futu_ktype(Period::Day), 2);
        assert_eq!(futu_autype(Market::AShare, Period::Day, 1), 1);
        assert_eq!(futu_autype(Market::Us, Period::Day, 1), 0);
    }

    #[test]
    fn decodes_eastmoney_utf8_and_gb18030() {
        let utf8 = "{\"name\":\"浦发银行\"}".as_bytes();
        assert!(decode_eastmoney_text(utf8).contains("浦发银行"));

        let (gbk, _, _) = encoding_rs::GB18030.encode("{\"name\":\"浦发银行\"}");
        assert!(decode_eastmoney_text(&gbk).contains("浦发银行"));
    }

    #[test]
    fn parses_leader_items() {
        let value = json!({
            "rc": 0,
            "data": {
                "tc": 51,
                "qdate": 20990101,
                "pool": [{
                    "c": "002548",
                    "m": 0,
                    "n": "金新农",
                    "p": 5010,
                    "zdp": 10.10988998413086,
                    "amount": 875234384,
                    "ltsz": 4030402174.9,
                    "tshare": 4149150056.64,
                    "hs": 22.573965,
                    "lbc": 4,
                    "fbt": 92500,
                    "lbt": 95724,
                    "fund": 101571432,
                    "zbc": 2,
                    "hybk": "饲料",
                    "zttj": { "days": 9, "ct": 6 }
                }]
            }
        });

        let items = parse_leader_items(&value).unwrap();
        assert_eq!(items.len(), 1);

        let item = &items[0];
        assert_eq!(item.symbol, "sz002548");
        assert_eq!(item.name, "金新农");
        assert!((item.price.unwrap() - 5.01).abs() < 1e-9);
        assert_eq!(item.boards, Some(4));
        assert_eq!(item.first_limit_time.as_deref(), Some("09:25:00"));
        assert_eq!(item.last_limit_time.as_deref(), Some("09:57:24"));
        assert_eq!(item.stat_days, Some(9));
        assert_eq!(item.stat_count, Some(6));
        assert_eq!(item.industry.as_deref(), Some("饲料"));
    }

    #[test]
    fn parses_empty_leader_items() {
        assert!(
            parse_leader_items(&json!({ "rc": 0, "data": null }))
                .unwrap()
                .is_empty()
        );
        assert!(parse_leader_items(&json!({ "rc": 102 })).is_err());
    }

    #[test]
    fn builds_industry_stats() {
        let value = json!({
            "rc": 0,
            "data": {
                "pool": [
                    { "c": "002548", "m": 0, "n": "金新农", "p": 5010, "lbc": 4,
                      "fund": 100.0, "amount": 1000.0, "hs": 20.0, "hybk": "饲料" },
                    { "c": "002001", "m": 0, "n": "饲料二", "p": 1000, "lbc": 2,
                      "fund": 50.0, "amount": 500.0, "hs": 10.0, "hybk": "饲料" },
                    { "c": "002579", "m": 0, "n": "中京电子", "p": 1782, "lbc": 6,
                      "fund": 200.0, "amount": 2000.0, "hs": 30.0, "hybk": "元件" }
                ]
            }
        });
        let items = parse_leader_items(&value).unwrap();
        let stats = build_industry_stats(&items);

        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].industry, "饲料");
        assert_eq!(stats[0].count, 2);
        assert!((stats[0].ratio - 200.0 / 3.0).abs() < 1e-9);
        assert_eq!(stats[0].max_boards, 4);
        assert_eq!(stats[0].total_boards, 6);
        assert_eq!(stats[0].seal_amount, 150.0);
        assert_eq!(stats[0].amount, 1500.0);
        assert!((stats[0].avg_turnover - 15.0).abs() < 1e-9);
        assert_eq!(stats[0].top_stock, "金新农");

        assert_eq!(stats[1].industry, "元件");
        assert_eq!(stats[1].count, 1);
        assert_eq!(stats[1].max_boards, 6);
        assert_eq!(stats[1].top_stock, "中京电子");
    }

    #[test]
    fn groups_missing_industry_as_other() {
        let value = json!({
            "rc": 0,
            "data": { "pool": [{ "c": "000001", "m": 0, "n": "测试", "hybk": "" }] }
        });
        let items = parse_leader_items(&value).unwrap();
        let stats = build_industry_stats(&items);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].industry, "其他");
        assert_eq!(stats[0].count, 1);
        assert_eq!(stats[0].max_boards, 0);
        assert_eq!(stats[0].avg_turnover, 0.0);
    }

    #[test]
    fn formats_leader_symbols_and_times() {
        assert_eq!(leader_symbol("600000", 1), "sh600000");
        assert_eq!(leader_symbol("000001", 0), "sz000001");
        assert_eq!(leader_symbol("920001", 0), "bj920001");
        assert_eq!(leader_symbol("430047", 0), "bj430047");

        assert_eq!(format_board_time(92500).as_deref(), Some("09:25:00"));
        assert_eq!(format_board_time(141141).as_deref(), Some("14:11:41"));
        assert_eq!(format_board_time(0), None);
        assert_eq!(format_board_time(240000), None);

        assert_eq!(format_leader_date("20261126"), "2026-11-26");
        assert_eq!(format_leader_date("unknown"), "unknown");
    }
}
