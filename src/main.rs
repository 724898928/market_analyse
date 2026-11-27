mod deriv;
mod market;
mod prediction;

use std::cmp::Ordering;
use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinSet;

use crate::deriv::{derivatives_sparse, moving_average as moving_average_series};
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
        .route("/api/predictions", get(api_predictions))
        .route("/api/watchlist", get(api_watchlist))
        .route("/api/watchlist/add", post(api_watch_add))
        .route("/api/watchlist/remove", delete(api_watch_remove))
        .route("/api/task", get(api_task))
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

async fn index() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    (headers, Html(include_str!("static/index.html")))
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

#[derive(Debug, Deserialize)]
struct PredictionQuery {
    symbol: Option<String>,
    period: Option<String>,
    source: Option<String>,
    mode: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct WatchQuery {
    symbol: Option<String>,
    name: Option<String>,
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
    d1: Vec<Option<f64>>,
    d2: Vec<Option<f64>>,
}

#[derive(Serialize)]
struct DerivOut {
    ma5: SeriesOut,
    ma10: SeriesOut,
    ma20: SeriesOut,
}

#[derive(Serialize)]
struct TaskSignal {
    current: f64,
    predicted: f64,
    direction: String,
}

#[derive(Serialize)]
struct TaskStock {
    code: String,
    symbol: String,
    name: String,
    price: Option<f64>,
    change_pct: Option<f64>,
    boards: Option<i64>,
    industry: Option<String>,
    ma5: TaskSignal,
    ma10: TaskSignal,
    ma20: TaskSignal,
}

#[derive(Serialize)]
struct TaskBoard {
    date: String,
    total: usize,
    matched: usize,
    items: Vec<TaskStock>,
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

async fn api_predictions(
    Query(query): Query<PredictionQuery>,
) -> Result<Json<prediction::PredictionHistory>, ApiError> {
    let symbol = query
        .symbol
        .unwrap_or_else(|| "sh600000".to_string())
        .trim()
        .to_string();
    if symbol.is_empty() {
        return Err(ApiError::BadRequest("请输入股票代码".to_string()));
    }
    let limit = query.limit.unwrap_or(120).clamp(10, 500);
    prediction::history(
        &symbol,
        query.period.as_deref(),
        query.source.as_deref(),
        query.mode.as_deref(),
        limit,
    )
    .map(Json)
    .map_err(ApiError::Upstream)
}

async fn api_watchlist() -> Result<Json<Vec<prediction::WatchItem>>, ApiError> {
    prediction::list_watch()
        .map(Json)
        .map_err(ApiError::Upstream)
}

async fn api_watch_add(
    Query(query): Query<WatchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let symbol = query.symbol.unwrap_or_default().trim().to_string();
    if symbol.is_empty() {
        return Err(ApiError::BadRequest("请输入股票代码".to_string()));
    }
    parse_symbol(&symbol).map_err(ApiError::BadRequest)?;
    prediction::add_watch(&symbol, query.name.as_deref().unwrap_or(""))
        .map_err(ApiError::Upstream)?;
    Ok(Json(json!({ "ok": true })))
}

async fn api_watch_remove(
    Query(query): Query<WatchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let symbol = query.symbol.unwrap_or_default().trim().to_string();
    if symbol.is_empty() {
        return Err(ApiError::BadRequest("请输入股票代码".to_string()));
    }
    parse_symbol(&symbol).map_err(ApiError::BadRequest)?;
    prediction::remove_watch(&symbol).map_err(ApiError::Upstream)?;
    Ok(Json(json!({ "ok": true })))
}

fn task_signal(bars: &[KlineBar], window: usize) -> Option<TaskSignal> {
    if bars.len() <= window {
        return None;
    }
    let closes: Vec<f64> = bars.iter().map(|bar| bar.close).collect();
    let ma = moving_average_series(&closes, window);
    let last = ma.len() - 1;
    let current = ma[last]?;
    let previous = ma[last - 1]?;
    if !current.is_finite() || !previous.is_finite() {
        return None;
    }
    let change = current - previous;
    let close_change = closes[last] - closes[last - 1];
    let damping = if change * close_change > 0.0 {
        1.0
    } else {
        0.3
    };
    let direction = if change > 1e-9 {
        "上行"
    } else if change < -1e-9 {
        "下行"
    } else {
        "持平"
    };
    Some(TaskSignal {
        current,
        predicted: current + change * damping,
        direction: direction.to_string(),
    })
}

async fn api_task(State(state): State<AppState>) -> Result<Json<TaskBoard>, ApiError> {
    let board = fetch_daily_leaders(&state.http, 1, 100, None)
        .await
        .map_err(ApiError::Upstream)?;
    let client = state.http.clone();
    let mut matched = Vec::new();

    for chunk in board.items.chunks(12) {
        let mut tasks = JoinSet::new();
        for leader in chunk {
            let symbol = leader.symbol.clone();
            let info = match parse_symbol(&symbol) {
                Ok(info) => info,
                Err(_) => continue,
            };
            let client = client.clone();
            let leader = leader.clone();
            tasks.spawn(async move {
                let data =
                    fetch_market_data(&client, &info, Period::Day, 60, 1, DataSource::Eastmoney)
                        .await
                        .ok()?;
                let ma5 = task_signal(&data.bars, 5)?;
                let ma10 = task_signal(&data.bars, 10)?;
                let ma20 = task_signal(&data.bars, 20)?;
                if ma5.direction == "上行" && ma10.direction == "下行" && ma20.direction == "下行"
                {
                    Some(TaskStock {
                        code: leader.code,
                        symbol: leader.symbol,
                        name: leader.name,
                        price: leader.price,
                        change_pct: leader.change_pct,
                        boards: leader.boards,
                        industry: leader.industry,
                        ma5,
                        ma10,
                        ma20,
                    })
                } else {
                    None
                }
            });
        }
        while let Some(result) = tasks.join_next().await {
            if let Ok(Some(item)) = result {
                matched.push(item);
            }
        }
    }

    matched.sort_by(|a, b| {
        b.boards
            .unwrap_or(0)
            .cmp(&a.boards.unwrap_or(0))
            .then_with(|| {
                b.change_pct
                    .unwrap_or(0.0)
                    .partial_cmp(&a.change_pct.unwrap_or(0.0))
                    .unwrap_or(Ordering::Equal)
            })
    });

    Ok(Json(TaskBoard {
        date: board.date,
        total: board.total,
        matched: matched.len(),
        items: matched,
    }))
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
    let _ = prediction::record_analysis(&symbol, &name);

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

    let closes: Vec<f64> = bars.iter().map(|bar| bar.close).collect();
    let series = |window: usize| -> Result<SeriesOut, ApiError> {
        let values = moving_average_series(&closes, window);
        let pairs = derivatives_sparse(&x_axis, &values, smooth).map_err(ApiError::BadRequest)?;
        Ok(SeriesOut {
            d1: pairs
                .iter()
                .map(|pair| (*pair).map(|value| value.first))
                .collect(),
            d2: pairs
                .iter()
                .map(|pair| (*pair).map(|value| value.second))
                .collect(),
        })
    };

    let deriv = DerivOut {
        ma5: series(5)?,
        ma10: series(10)?,
        ma20: series(20)?,
    };

    let series_data = [("ma5", 5usize), ("ma10", 10usize), ("ma20", 20usize)];
    if !symbol.eq_ignore_ascii_case("demo") {
        let _ = prediction::record_predictions(
            &symbol,
            period.klt(),
            source,
            mode,
            &bars,
            &x_axis,
            &series_data,
        );
    }

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
