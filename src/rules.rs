//! Rule-based policy triggers. They feed the LLM as candidates and are the fallback answer
//! when no API key is set or the LLM output fails validation.

use crate::facts::Facts;
use crate::policies;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Trigger {
    pub policy_id: &'static str,
    pub score: f64,
    pub reasons: Vec<String>,
    pub evidence: Vec<String>,
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 || v.abs() >= 100.0 {
        let s = format!("{:.0}", v);
        // thousands separator
        let (sign, d) = s.split_at(if s.starts_with('-') { 1 } else { 0 });
        let mut o = String::new();
        for (i, c) in d.chars().enumerate() {
            if i > 0 && (d.len() - i) % 3 == 0 {
                o.push(',');
            }
            o.push(c);
        }
        format!("{sign}{o}")
    } else {
        format!("{v:.1}")
    }
}

/// " in a 1-in-100-year flood" for ids like "flood.rp100.x" / "region.rp100.x".
fn scen(id: &str) -> String {
    match id.split('.').nth(1).filter(|k| k.starts_with("rp") || *k == "observed") {
        Some(k) => format!(" in a {} flood", crate::facts::scenario_label(k)),
        None => String::new(),
    }
}

pub fn run(f: &Facts) -> Vec<Trigger> {
    let mut t: Vec<Trigger> = Vec::new();
    let mut add = |id: &'static str, score: f64, reason: String, ev: &str| {
        if let Some(x) = t.iter_mut().find(|x| x.policy_id == id) {
            x.score += score;
            x.reasons.push(reason);
            x.evidence.push(ev.to_string());
        } else {
            t.push(Trigger { policy_id: id, score, reasons: vec![reason], evidence: vec![ev.to_string()] });
        }
    };

    if let Some((id, v)) = f.max_suffix(".people_flooded_or_cut_off").filter(|x| x.1 > 0.0) {
        add("ST-ALERT", 3.0, format!("{} people would be flooded or cut off{}", fmt(v), scen(&id)), &id);
        add("ST-COMMS", 2.0, format!("{} people need to know what to do", fmt(v)), &id);
    }
    if let Some((id, v)) = f.max_suffix(".people_leave_now").filter(|x| x.1 > 0.0) {
        add("ST-EVACAPP", 3.0, format!("{} people must leave now{}", fmt(v), scen(&id)), &id);
        add("ST-ALERT", 2.0, format!("{} people must leave now, so warnings must arrive early", fmt(v)), &id);
    }
    for suf in [".people_boat_or_air", "evacuation.max_boat_or_air", "timeline.peak_people_air"] {
        if let Some((id, v)) = f.max_suffix(suf).filter(|x| x.1 > 0.0) {
            add("ST-PREP", 3.0, format!("{} people reachable only by boat or air{}: pre-position rescue means", fmt(v), scen(&id)), &id);
            add("ST-CRISIS", 2.0, format!("{} people need boat or air rescue{}: coordinate reinforcements", fmt(v), scen(&id)), &id);
            break; // the first source (flood scenarios) is enough
        }
    }
    if let Some(v) = f.get("evacuation.max_need_transport").filter(|v| *v > 0.0) {
        add("ST-INCLUSIVE", 3.0, format!("{} evacuees need transport", fmt(v)), "evacuation.max_need_transport");
    }
    if let Some(v) = f.get("evacuation.max_time_to_safety_p95_h").filter(|v| *v > 0.0) {
        let s = if v >= 2.0 { 3.0 } else { 1.0 };
        add("ST-EVACAPP", s, format!("95% of evacuees safe only after {} h", fmt(v)), "evacuation.max_time_to_safety_p95_h");
        if v >= 2.0 {
            add("ST-ALERT", 2.0, format!("evacuation takes {} h, longer than a flash-flood warning window", fmt(v)), "evacuation.max_time_to_safety_p95_h");
        }
    }
    for suf in [".roads_closed_km", "evacuation.max_roads_congested_km"] {
        if let Some((id, v)) = f.max_suffix(suf).filter(|x| x.1 > 0.0) {
            add("ST-EVACAPP", 1.5, format!("{} km of road closed or jammed{}: route around it", fmt(v), scen(&id)), &id);
        }
    }
    if let Some(v) = f.get("evacuation.buildings_cut_off").filter(|v| *v > 0.0) {
        add("ST-INCLUSIVE", 1.5, format!("{} buildings cut off: people there depend on others", fmt(v)), "evacuation.buildings_cut_off");
    }

    // care facilities / critical infrastructure in the area
    for fact in &f.0 {
        let Some(k) = fact.id.strip_prefix("region.facilities.") else { continue };
        let Some(n) = fact.value.as_f64().filter(|n| *n > 0.0) else { continue };
        let k = k.to_lowercase();
        if k.contains("hospital") || k.contains("nursing") || k.contains("care") || k.contains("elder") {
            add("ST-CAREHOMES", 3.0, format!("{}: {} in the area", k.replace('_', " "), fmt(n)), &fact.id);
            add("ST-PREP", 1.5, format!("{}: {} to keep running", k.replace('_', " "), fmt(n)), &fact.id);
        } else if k.contains("fire") || k.contains("police") || k.contains("power") || k.contains("water") {
            add("ST-PREP", 1.0, format!("{}: {} in the area", k.replace('_', " "), fmt(n)), &fact.id);
        } else if k.contains("school") || k.contains("shelter") || k.contains("stadium") || k.contains("sport") {
            add("ST-EVACAPP", 0.5, format!("{}: {} usable as shelters", k.replace('_', " "), fmt(n)), &fact.id);
        }
    }
    if let Some(v) = f.get("region.people_in_high_risk").filter(|v| *v > 0.0) {
        add("ST-ALERT", 1.5, format!("~{} people live in high-risk cells", fmt(v)), "region.people_in_high_risk");
        add("LT-ZONING", 2.0, format!("~{} people live in high-risk cells", fmt(v)), "region.people_in_high_risk");
    }
    if let Some(v) = f.get("region.high_risk_km2").filter(|v| *v > 0.0) {
        add("ST-MITIG", 1.5, format!("{} km² at high risk should shape land-use plans", fmt(v)), "region.high_risk_km2");
        add("LT-RETENTION", 1.0, format!("{} km² at high risk", fmt(v)), "region.high_risk_km2");
    }

    if let Some((id, v)) = f.max_suffix(".buildings_flooded").filter(|x| x.1 > 0.0) {
        add("LT-RECOVERY", 2.0, format!("{} buildings would flood{}", fmt(v), scen(&id)), &id);
        add("ST-MITIG", 1.5, format!("{} buildings exposed{}", fmt(v), scen(&id)), &id);
        add("LT-ZONING", 1.0, format!("{} buildings exposed{}", fmt(v), scen(&id)), &id);
    }
    if let Some(v) = f.get("impact.loss_m_eur_2021").filter(|v| *v > 0.0) {
        add("LT-RECOVERY", 1.5, format!("~€{}M building loss in July 2021 (all EMS map areas)", fmt(v)), "impact.loss_m_eur_2021");
        add("ST-MITIG", 1.0, format!("~€{}M building loss in July 2021 (all EMS map areas)", fmt(v)), "impact.loss_m_eur_2021");
    }
    if let Some(v) = f.get("impact.displaced_2021").filter(|v| *v > 0.0) {
        add("LT-ZONING", 1.0, format!("~{} people displaced in July 2021 (all EMS map areas)", fmt(v)), "impact.displaced_2021");
    }

    // weak model fit → more monitoring, less reliance on the map alone
    let pct = f.get("region.cells_outside_training_pct").unwrap_or(0.0);
    let auc = f.get("region.nearest_training_aoi_auc").unwrap_or(1.0);
    if pct > 10.0 || auc < 0.7 {
        let (id, r) = if pct > 10.0 {
            ("region.cells_outside_training_pct", format!("{}% of cells are outside the model's training range: monitor on the ground", fmt(pct)))
        } else {
            ("region.nearest_training_aoi_auc", format!("nearest training area held-out AUC is only {}: do not rely on the map alone", fmt(auc)))
        };
        add("ST-AI", 2.0, r, id);
    }
    if f.has("models.eval0") {
        add("ST-AI", 0.5, "model checks are available and should be shown with every decision".into(), "models.eval0");
    }
    if let Some(ev) = t.first().map(|x| x.evidence[0].clone()) {
        t.push(Trigger {
            policy_id: "LT-AWARE",
            score: 0.5,
            reasons: vec!["people in exposed areas need to know the risk before the next event".into()],
            evidence: vec![ev],
        });
    }

    t.retain(|x| policies::get(x.policy_id).is_some());
    t.sort_by(|a, b| b.score.total_cmp(&a.score));
    t
}
