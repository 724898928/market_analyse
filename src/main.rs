mod deriv;
mod market;

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::deriv::{derivatives, smooth as smooth_series};
use crate::market::{
    DataSource, KlineBar, LeaderBoard, Market, Period, Quote, demo_kline, demo_quote,
    fetch_daily_leaders, fetch_market_data, fetch_quote, futu_configured, parse_symbol,
};

#[derive(Clone)]
struct AppState {
    http: reqwest::Client,
}

#[tokio::main]
async fn main() {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(
        header::REFERER,
        HeaderValue::from_static("https://quote.eastmoney.com/"),
    );
    default_headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    default_headers.insert(
        header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"),
    );

    let http = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .default_headers(default_headers)
        .timeout(Duration::from_secs(15))
        .build()
        .expect("创建 HTTP 客户端失败");

    let app = Router::new()
        .route("/", get(index))
        .route("/assets/echarts.min.js", get(echarts_js))
        .route("/api/kline", get(api_kline))
        .route("/api/quote", get(api_quote))
        .route("/api/leaders", get(api_leaders))
        .with_state(AppState { http });

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|error| panic!("无法监听 {addr}: {error}"));

    println!("K线导数分析器已启动: http://{addr}");
    if futu_configured() {
        println!("富途牛牛通道: 已配置（FUTU_ACCESS_TOKEN / FUTU_TOKEN）");
    } else {
        println!("富途牛牛通道: 未配置（设置 FUTU_ACCESS_TOKEN 后可在数据源中选择）");
    }
    println!("按 Ctrl+C 停止服务");

    axum::serve(listener, app).await.expect("Web 服务运行失败");
}

async fn index() -> Html<&'static str> {
    Html(include_str!("static/index.html"))
}

async fn echarts_js() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    (headers, include_str!("static/echarts.min.js"))
}

#[derive(Debug, Deserialize)]
struct KlineQuery {
    symbol: Option<String>,
    period: Option<String>,
    limit: Option<usize>,
    adjust: Option<u8>,
    smooth: Option<usize>,
    mode: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuoteQuery {
    symbol: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LeadersQuery {
    page: Option<usize>,
    page_size: Option<usize>,
    industry: Option<String>,
}

enum ApiError {
    BadRequest(String),
    Upstream(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Upstream(message) => (StatusCode::BAD_GATEWAY, message),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[derive(Serialize)]
struct KlineResponse {
    symbol: String,
    name: String,
    period: String,
    period_label: String,
    adjust: u8,
    adjust_requested: u8,
    adjust_note: Option<String>,
    smooth: usize,
    mode: String,
    unit: String,
    unit2: String,
    source: String,
    bars: Vec<BarOut>,
    deriv: DerivOut,
    quote: Option<Quote>,
}

#[derive(Serialize)]
struct BarOut {
    t: i64,
    date: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Serialize)]
struct SeriesOut {
    d1: Vec<f64>,
    d2: Vec<f64>,
}

#[derive(Serialize)]
struct DerivOut {
    open: SeriesOut,
    high: SeriesOut,
    low: SeriesOut,
    close: SeriesOut,
}

async fn api_kline(
    State(state): State<AppState>,
    Query(query): Query<KlineQuery>,
) -> Result<Json<KlineResponse>, ApiError> {
    let symbol = query
        .symbol
        .unwrap_or_else(|| "sh600000".to_string())
        .trim()
        .to_string();
    if symbol.is_empty() {
        return Err(ApiError::BadRequest("请输入股票代码".to_string()));
    }
    let source = DataSource::parse(query.source.as_deref()).map_err(ApiError::BadRequest)?;

    let period =
        Period::parse(query.period.as_deref().unwrap_or("101")).map_err(ApiError::BadRequest)?;
    let limit = query.limit.unwrap_or(300).clamp(3, 2000);
    let adjust = query.adjust.unwrap_or(1);
    if adjust > 2 {
        return Err(ApiError::BadRequest("复权参数只能是 0、1 或 2".to_string()));
    }
    let smooth = query.smooth.unwrap_or(1).clamp(1, 60);
    let mode = query.mode.unwrap_or_else(|| "time".to_string());
    if mode != "time" && mode != "index" && mode != "trading" {
        return Err(ApiError::BadRequest(
            "mode 只能是 time、trading 或 index".to_string(),
        ));
    }

    if symbol.eq_ignore_ascii_case("demo") {
        let data = demo_kline(limit);
        let quote = Some(demo_quote(&data));
        let adjust_note = (adjust != 0).then(|| "模拟数据不涉及复权".to_string());
        return build_response(
            symbol,
            data.name,
            "demo",
            period,
            0,
            adjust,
            adjust_note,
            smooth,
            &mode,
            data.bars,
            quote,
        )
        .map(Json);
    }

    let symbol_info = parse_symbol(&symbol).map_err(ApiError::BadRequest)?;
    let (adjust_applied, adjust_note) = effective_adjust(symbol_info.market, period, adjust);
    let data = fetch_market_data(
        &state.http,
        &symbol_info,
        period,
        limit,
        adjust_applied,
        source,
    )
    .await
    .map_err(ApiError::Upstream)?;

    build_response(
        symbol,
        data.name,
        data.source,
        period,
        adjust_applied,
        adjust,
        adjust_note,
        smooth,
        &mode,
        data.bars,
        data.quote,
    )
    .map(Json)
}

fn effective_adjust(market: Market, period: Period, requested: u8) -> (u8, Option<String>) {
    if market == Market::AShare && !period.is_intraday() {
        return (requested, None);
    }
    if requested == 0 {
        return (0, None);
    }
    if period.is_intraday() {
        (0, Some("分钟K线仅提供不复权数据".to_string()))
    } else {
        (0, Some("港股/美股仅提供不复权数据".to_string()))
    }
}

async fn api_quote(
    State(state): State<AppState>,
    Query(query): Query<QuoteQuery>,
) -> Result<Json<Quote>, ApiError> {
    let symbol = query
        .symbol
        .unwrap_or_else(|| "sh600000".to_string())
        .trim()
        .to_string();
    if symbol.is_empty() {
        return Err(ApiError::BadRequest("请输入股票代码".to_string()));
    }
    let source = DataSource::parse(query.source.as_deref()).map_err(ApiError::BadRequest)?;
    if symbol.eq_ignore_ascii_case("demo") {
        return Ok(Json(demo_quote(&demo_kline(120))));
    }
    let symbol_info = parse_symbol(&symbol).map_err(ApiError::BadRequest)?;
    fetch_quote(&state.http, &symbol_info, source)
        .await
        .map(Json)
        .map_err(ApiError::Upstream)
}

async fn api_leaders(
    State(state): State<AppState>,
    Query(query): Query<LeadersQuery>,
) -> Result<Json<LeaderBoard>, ApiError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(10).clamp(1, 50);
    fetch_daily_leaders(&state.http, page, page_size, query.industry.as_deref())
        .await
        .map(Json)
        .map_err(ApiError::Upstream)
}

#[allow(clippy::too_many_arguments)]
fn build_response(
    symbol: String,
    name: String,
    source: &str,
    period: Period,
    adjust: u8,
    adjust_requested: u8,
    adjust_note: Option<String>,
    smooth: usize,
    mode: &str,
    bars: Vec<KlineBar>,
    quote: Option<Quote>,
) -> Result<KlineResponse, ApiError> {
    if bars.len() < 3 {
        return Err(ApiError::Upstream(
            "有效K线数量不足，至少需要3根".to_string(),
        ));
    }

    let (unit, x_axis) = match mode {
        "index" => (
            "根".to_string(),
            (0..bars.len())
                .map(|index| index as f64)
                .collect::<Vec<_>>(),
        ),
        "trading" => {
            let (unit, step) = if period.is_intraday() {
                ("分钟".to_string(), period.minutes() as f64)
            } else {
                let unit = match period {
                    Period::Day => "交易日",
                    Period::Week => "周",
                    Period::Month => "月",
                    _ => unreachable!(),
                };
                (unit.to_string(), 1.0)
            };
            (
                unit,
                (0..bars.len())
                    .map(|index| index as f64 * step)
                    .collect::<Vec<_>>(),
            )
        }
        _ => {
            let divisor = period.time_divisor();
            (
                period.time_unit().to_string(),
                bars.iter()
                    .map(|bar| bar.time_ms as f64 / 1000.0 / divisor)
                    .collect::<Vec<_>>(),
            )
        }
    };

    let series = |get: fn(&KlineBar) -> f64| -> Result<SeriesOut, ApiError> {
        let raw: Vec<f64> = bars.iter().map(get).collect();
        let values = smooth_series(&raw, smooth);
        let pairs = derivatives(&x_axis, &values).map_err(ApiError::BadRequest)?;
        Ok(SeriesOut {
            d1: pairs.iter().map(|pair| pair.first).collect(),
            d2: pairs.iter().map(|pair| pair.second).collect(),
        })
    };

    let deriv = DerivOut {
        open: series(|bar| bar.open)?,
        high: series(|bar| bar.high)?,
        low: series(|bar| bar.low)?,
        close: series(|bar| bar.close)?,
    };

    let out_bars = bars
        .into_iter()
        .map(|bar| BarOut {
            t: bar.time_ms,
            date: bar.label,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .collect();

    Ok(KlineResponse {
        symbol,
        name,
        period: period.klt().to_string(),
        period_label: period.label().to_string(),
        adjust,
        adjust_requested,
        adjust_note,
        smooth,
        mode: mode.to_string(),
        unit2: format!("{unit}²"),
        unit,
        source: source.to_string(),
        bars: out_bars,
        deriv,
        quote,
    })
}
