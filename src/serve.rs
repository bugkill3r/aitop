use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use tiny_http::{Header, Method, Response, Server};

use crate::data::aggregator::Aggregator;
use crate::data::pricing::PricingRegistry;
use crate::ui::format::shorten_model;

#[derive(Serialize)]
struct ApiStats {
    spend_today: f64,
    spend_this_week: f64,
    spend_all_time: f64,
    burn_rate: f64,
    is_live: bool,
    cache_hit_ratio: f64,
    top_models: Vec<ApiModel>,
    recent_sessions: Vec<ApiSession>,
}

#[derive(Serialize)]
struct ApiModel {
    model: String,
    cost: f64,
    percentage: f64,
}

#[derive(Serialize)]
struct ApiSession {
    project: String,
    model: String,
    cost: f64,
    updated_at: String,
}

pub fn run(db_path: &Path, pricing: &PricingRegistry, port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("{}", e))?;
    eprintln!("aitop API listening on http://{}", addr);

    let cors: Header = "Access-Control-Allow-Origin: *"
        .parse()
        .expect("valid header");
    let content_type: Header = "Content-Type: application/json"
        .parse()
        .expect("valid header");
    let allow_methods: Header = "Access-Control-Allow-Methods: GET, OPTIONS"
        .parse()
        .expect("valid header");
    let allow_headers: Header = "Access-Control-Allow-Headers: Content-Type"
        .parse()
        .expect("valid header");

    for request in server.incoming_requests() {
        if request.method() == &Method::Options {
            let mut resp = Response::from_string("");
            resp.add_header(cors.clone());
            resp.add_header(allow_methods.clone());
            resp.add_header(allow_headers.clone());
            let _ = request.respond(resp);
            continue;
        }

        if request.url() != "/api/stats" {
            let mut resp = Response::from_string(r#"{"error":"not found"}"#).with_status_code(404);
            resp.add_header(cors.clone());
            resp.add_header(content_type.clone());
            let _ = request.respond(resp);
            continue;
        }

        match build_stats(db_path, pricing) {
            Ok(json) => {
                let mut resp = Response::from_string(json);
                resp.add_header(cors.clone());
                resp.add_header(content_type.clone());
                let _ = request.respond(resp);
            }
            Err(e) => {
                let body = serde_json::json!({"error": e.to_string()}).to_string();
                let mut resp = Response::from_string(body).with_status_code(500);
                resp.add_header(cors.clone());
                resp.add_header(content_type.clone());
                let _ = request.respond(resp);
            }
        }
    }

    Ok(())
}

fn build_stats(db_path: &Path, pricing: &PricingRegistry) -> Result<String> {
    let agg = Aggregator::open_with_pricing(db_path, pricing.clone())?;

    let stats = agg.dashboard_stats()?;
    let models = agg.model_breakdown()?;
    let sessions = agg.sessions_list(5)?;
    let cache_ratio = agg.cache_hit_ratio()?;
    let is_live = agg.is_live()?;

    let total_cost: f64 = models.iter().map(|m| m.cost).sum();

    let top_models: Vec<ApiModel> = models
        .iter()
        .take(5)
        .map(|m| ApiModel {
            model: m.model.clone(),
            cost: round2(m.cost),
            percentage: if total_cost > 0.0 {
                round1(m.cost / total_cost * 100.0)
            } else {
                0.0
            },
        })
        .collect();

    let recent_sessions: Vec<ApiSession> = sessions
        .iter()
        .map(|s| ApiSession {
            project: s.project.clone(),
            model: shorten_model(&s.model),
            cost: round2(s.total_cost),
            updated_at: s.updated_at.clone(),
        })
        .collect();

    let api_stats = ApiStats {
        spend_today: round2(stats.spend_today),
        spend_this_week: round2(stats.spend_this_week),
        spend_all_time: round2(stats.spend_all_time),
        burn_rate: round2(stats.burn_rate_per_hour),
        is_live,
        cache_hit_ratio: round1(cache_ratio * 100.0),
        top_models,
        recent_sessions,
    };

    Ok(serde_json::to_string(&api_stats)?)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
