//! Turn flood_api model responses (as the frontend received them) into a flat list of facts.
//! The LLM may only cite these facts; the validator checks its numbers against them.

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Serialize, Clone)]
pub struct Fact {
    pub id: String,
    pub text: String,
    pub value: Value,
}

#[derive(Default)]
pub struct Facts(pub Vec<Fact>);

impl Facts {
    fn num(&mut self, id: String, text: &str, v: Option<f64>) {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            let v = (v * 100.0).round() / 100.0;
            self.0.push(Fact { id, text: text.into(), value: v.into() });
        }
    }
    fn str(&mut self, id: String, text: &str, v: Option<&str>) {
        if let Some(v) = v.filter(|s| !s.is_empty()) {
            self.0.push(Fact { id, text: text.into(), value: v.into() });
        }
    }
    pub fn get(&self, id: &str) -> Option<f64> {
        self.0.iter().find(|f| f.id == id).and_then(|f| f.value.as_f64())
    }
    /// Largest value over all facts whose id ends with `suffix` (e.g. across flood scenarios).
    pub fn max_suffix(&self, suffix: &str) -> Option<(String, f64)> {
        self.0
            .iter()
            .filter(|f| f.id.ends_with(suffix))
            .filter_map(|f| f.value.as_f64().map(|v| (f.id.clone(), v)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }
    pub fn has(&self, id: &str) -> bool {
        self.0.iter().any(|f| f.id == id)
    }
}

fn f(v: &Value, path: &str) -> Option<f64> {
    path.split('.').try_fold(v, |v, k| v.get(k))?.as_f64()
}

fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn max_of(rows: &[Value], k: &str) -> Option<(f64, usize)> {
    rows.iter()
        .enumerate()
        .filter_map(|(i, r)| r.get(k)?.as_f64().map(|v| (v, i)))
        .max_by(|a, b| a.0.total_cmp(&b.0))
}

pub fn extract(models: &Map<String, Value>) -> Facts {
    let mut out = Facts::default();
    if let Some(j) = models.get("flood") {
        out.num("flood.area_km2".into(), "area checked (km²)", f(j, "region_km2"));
        out.num("flood.outside_study_region_pct".into(), "% of the area outside the region the models were built on", f(j, "outside_study_region_pct"));
        if let Some(fl) = j.get("floods").and_then(Value::as_object) {
            for (k, s) in fl {
                let p = |x: &str| format!("flood.{k}.{x}");
                let t = |x: &str| format!("{x}, {} flood", scenario_label(k));
                out.num(p("people_flooded_or_cut_off"), &t("people flooded or cut off"), f(s, "people_flooded_or_cut_off"));
                out.num(p("people_leave_now"), &t("people who must leave now"), f(s, "evacuation.people_now"));
                out.num(p("people_leave_early"), &t("people who should leave early"), f(s, "evacuation.people_early"));
                out.num(p("people_boat_or_air"), &t("people reachable only by boat or air"), f(s, "evacuation.people_by_boat_or_air"));
                out.num(p("buildings_flooded"), &t("buildings flooded"), f(s, "buildings_flooded"));
                out.num(p("roads_closed_km"), &t("km of road closed"), f(s, "roads_closed_km"));
            }
        }
    }
    if let Some(j) = models.get("evacuation") {
        out.num("evacuation.buildings_flooded".into(), "buildings flooded (evacuation run)", f(j, "buildings_flooded"));
        out.num("evacuation.buildings_cut_off".into(), "buildings cut off by water", f(j, "buildings_cut_off"));
        let runs = arr(j, "runs");
        for (k, label) in [
            ("evacuees", "evacuees"),
            ("need_transport", "evacuees who need transport"),
            ("boat_or_air", "evacuees needing boat or air rescue"),
            ("roads_congested_km", "km of road jammed"),
            ("time_to_safety_p95_h", "hours until 95% of evacuees are safe"),
        ] {
            if let Some((v, i)) = max_of(runs, k) {
                let run = runs[i].get("run").and_then(Value::as_str).unwrap_or("?");
                out.num(format!("evacuation.max_{k}"), &format!("{label} (worst run: {run})"), Some(v));
            }
        }
    }
    if let Some(j) = models.get("timeline") {
        let steps = arr(j, "steps");
        for (k, label) in [
            ("flooded_km2", "km² flooded at the peak"),
            ("buildings_flooded", "buildings flooded at the peak"),
            ("roads_closed_km", "km of road closed at the peak"),
            ("people_now", "people who must leave now at the peak"),
            ("people_air", "people reachable only by boat or air at the peak"),
        ] {
            if let Some((v, i)) = max_of(steps, k) {
                out.num(format!("timeline.peak_{k}"), label, Some(v));
                if k == "people_now" {
                    let t = steps[i].get("time").and_then(Value::as_str);
                    out.str("timeline.peak_time".into(), "time of the peak (what-if flood)", t);
                }
            }
        }
    }
    if let Some(j) = models.get("region") {
        out.num("region.area_km2".into(), "area checked (km²)", f(j, "area_km2"));
        out.num("region.high_risk_km2".into(), "km² at high model risk", f(j, "risk.high_risk_km2"));
        out.num("region.people_in_high_risk".into(), "people in high-risk cells (approx.)", f(j, "risk.people_in_high_risk_approx"));
        for s in arr(j.get("risk").unwrap_or(&Value::Null), "by_scenario") {
            let k = s.get("scenario").and_then(Value::as_str).unwrap_or("?");
            out.num(format!("region.{k}.people_in_wet_cells"), &format!("people in wet cells, {} flood", scenario_label(k)), f(s, "people_approx"));
            out.num(format!("region.{k}.deep_km2"), &format!("km² with water ≥ 0.3 m, {} flood", scenario_label(k)), f(s, "deep_km2"));
            out.num(format!("region.{k}.max_depth_m"), &format!("max water depth (m), {} flood", scenario_label(k)), f(s, "max_depth_m"));
        }
        if let Some(fac) = j.get("facilities").and_then(Value::as_object) {
            for (k, v) in fac {
                let n = v.as_f64().or_else(|| v.as_array().map(|a| a.len() as f64));
                out.num(format!("region.facilities.{k}"), &format!("{} inside the area", k.replace('_', " ")), n);
            }
        }
        out.num("region.cells_outside_training_pct".into(), "% of cells outside the model's training range", f(j, "region_check.cells_outside_training_range_pct"));
        out.num("region.nearest_training_aoi_auc".into(), "held-out ROC AUC of the nearest training area", f(j, "region_check.nearest_training_aoi_held_out_auc"));
        let a = j.pointer("/region_check/nearest_training_aoi").and_then(Value::as_str);
        out.str("region.nearest_training_aoi".into(), "nearest training area (EMS AOI)", a);
    }
    if let Some(j) = models.get("impact") {
        let aois = arr(j, "aois");
        let sum = |k: &str| aois.iter().filter_map(|a| a.get(k)?.as_f64()).sum::<f64>();
        if !aois.is_empty() {
            out.num("impact.aois".into(), "July 2021 EMS map areas with damage data", Some(aois.len() as f64));
            out.num("impact.buildings_flooded_2021".into(), "buildings flooded in July 2021 (EMS maps)", Some(sum("flooded")));
            out.num("impact.loss_m_eur_2021".into(), "estimated building loss in July 2021 (million €, assumed rebuild cost)", Some(sum("loss_m")));
            out.num("impact.displaced_2021".into(), "people displaced in July 2021 (estimate)", Some(sum("displaced")));
        }
    }
    if let Some(j) = models.get("models") {
        // evaluation values are strings like "0.922 / 0.255 (base rate 0.010)"; keep them as text facts
        for (i, r) in arr(j, "evaluation").iter().enumerate() {
            let txt = format!(
                "{} | {} | {}",
                r.get("check").and_then(Value::as_str).unwrap_or(""),
                r.get("model").and_then(Value::as_str).unwrap_or(""),
                r.get("metric").and_then(Value::as_str).unwrap_or("")
            );
            let v = r.get("value").cloned().unwrap_or(Value::Null);
            out.0.push(Fact { id: format!("models.eval{i}"), text: txt, value: v });
        }
    }
    out
}

pub fn scenario_label(k: &str) -> String {
    match k {
        "rp10" => "1-in-10-year".into(),
        "rp100" => "1-in-100-year".into(),
        "rp500" => "1-in-500-year".into(),
        "observed" => "July 2021 (satellite)".into(),
        k => k.replace("risk_ge_", "model risk ≥ "),
    }
}

/// Caveats the flood_api attached to its responses, deduplicated.
pub fn caveats(models: &Map<String, Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in models.values() {
        for c in arr(v, "caveats").iter().filter_map(Value::as_str) {
            if !out.iter().any(|o| o == c) {
                out.push(c.to_string());
            }
        }
    }
    out
}
