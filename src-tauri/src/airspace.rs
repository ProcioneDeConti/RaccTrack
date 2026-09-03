//! US airspace polygons from the FAA's ArcGIS open-data feature services
//! (Aeronautical Information Services). Public, no key. Queried by viewport
//! bounding box, normalized to a small GeoJSON FeatureCollection, cached ~7 days
//! per coarse tile in `kv_cache`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::db::Db;
use crate::region::Area;

const TTL_MS: i64 = 7 * 24 * 3600 * 1000;
const ARCGIS: &str =
    "https://services6.arcgis.com/ssFJjBXIUyZDrSYZ/arcgis/rest/services";

pub struct Airspace {
    db: Arc<Db>,
    client: reqwest::Client,
}

impl Airspace {
    pub fn new(db: Arc<Db>, client: reqwest::Client) -> Self {
        Self { db, client }
    }

    /// GeoJSON FeatureCollection of class + special-use airspace intersecting
    /// the area. Each feature's `properties` has `category`, `name`, `lower`,
    /// `upper`, `lowerFt`, `upperFt`, `times`.
    pub async fn in_area(&self, area: Area) -> Result<Value> {
        let a = area.clamped();
        // Snap to a 0.5-degree grid so panning reuses cached tiles.
        let (w, s, e, n) = (
            (a.west * 2.0).floor() / 2.0,
            (a.south * 2.0).floor() / 2.0,
            (a.east * 2.0).ceil() / 2.0,
            (a.north * 2.0).ceil() / 2.0,
        );
        let key = format!("airspace:{w:.1},{s:.1},{e:.1},{n:.1}");
        if let Some(json) = self.db.kv_get(&key, TTL_MS)? {
            if let Ok(v) = serde_json::from_str(&json) {
                return Ok(v);
            }
        }

        let bbox = format!("{w},{s},{e},{n}");
        let mut features = Vec::new();
        match self.query("Class_Airspace", &bbox).await {
            Ok(mut f) => features.append(&mut f),
            Err(e) => tracing::debug!("class airspace query failed: {e}"),
        }
        match self.query("Special_Use_Airspace", &bbox).await {
            Ok(mut f) => features.append(&mut f),
            Err(e) => tracing::debug!("SUA query failed: {e}"),
        }

        if features.is_empty() {
            if let Some(stale) = self.db.kv_get_stale(&key)? {
                if let Ok(v) = serde_json::from_str(&stale) {
                    return Ok(v);
                }
            }
        }

        let fc = json!({ "type": "FeatureCollection", "features": features });
        let _ = self.db.kv_put(&key, &serde_json::to_string(&fc)?);
        Ok(fc)
    }

    async fn query(&self, service: &str, bbox: &str) -> Result<Vec<Value>> {
        let url = format!(
            "{ARCGIS}/{service}/FeatureServer/0/query\
             ?where=1%3D1&geometry={bbox}&geometryType=esriGeometryEnvelope\
             &inSR=4326&outSR=4326&spatialRel=esriSpatialRelIntersects\
             &outFields=*&returnGeometry=true&maxAllowableOffset=0.002\
             &resultRecordCount=400&f=geojson"
        );
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(25))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("{service} HTTP {}", resp.status()));
        }
        let fc: Value = resp.json().await?;
        let arr = fc
            .get("features")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(arr.into_iter().filter_map(normalize).collect())
    }
}

/// Reduce a raw FAA feature to the fields the map needs.
fn normalize(mut feat: Value) -> Option<Value> {
    let p = feat.get("properties")?.clone();
    let get = |k: &str| p.get(k).cloned().unwrap_or(Value::Null);
    let as_str = |v: &Value| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };

    let class = as_str(&get("CLASS"));
    let local = as_str(&get("LOCAL_TYPE")).unwrap_or_default().to_uppercase();
    let type_code = as_str(&get("TYPE_CODE")).unwrap_or_default().to_uppercase();

    let category = if type_code == "MOA" {
        "MOA"
    } else if matches!(type_code.as_str(), "R" | "RESTRICTED") {
        "RESTRICTED"
    } else if matches!(type_code.as_str(), "P" | "PROHIBITED") {
        "PROHIBITED"
    } else if matches!(type_code.as_str(), "W" | "WARNING") {
        "WARNING"
    } else if matches!(type_code.as_str(), "A" | "ALERT") {
        "ALERT"
    } else if local.contains("MODE") {
        "MODE_C"
    } else {
        match class.as_deref() {
            Some("B") => "CLASS_B",
            Some("C") => "CLASS_C",
            Some("D") => "CLASS_D",
            Some("E") => "CLASS_E",
            _ => return None, // skip Class E5 sheets, unknowns
        }
    };

    let lower_ft = to_feet(&get("LOWER_VAL"), &get("LOWER_UOM"));
    let upper_ft = to_feet(&get("UPPER_VAL"), &get("UPPER_UOM"));

    let props = json!({
        "category": category,
        "name": as_str(&get("NAME")),
        "lower": label(&get("LOWER_VAL"), &get("LOWER_UOM"), &get("LOWER_CODE")),
        "upper": label(&get("UPPER_VAL"), &get("UPPER_UOM"), &get("UPPER_CODE")),
        "lowerFt": lower_ft,
        "upperFt": upper_ft,
        "times": as_str(&get("TIMESOFUSE")),
        "agent": as_str(&get("CONT_AGENT")),
    });
    feat["properties"] = props;
    Some(feat)
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn to_feet(val: &Value, uom: &Value) -> Option<f64> {
    let n = num(val)?;
    if n <= -9990.0 {
        return None; // FAA sentinel for surface / unlimited
    }
    let u = match uom {
        Value::String(s) => s.to_uppercase(),
        _ => "FT".into(),
    };
    Some(if u == "FL" { n * 100.0 } else { n })
}

fn label(val: &Value, uom: &Value, code: &Value) -> Option<String> {
    let n = num(val)?;
    let u = match uom {
        Value::String(s) => s.to_uppercase(),
        _ => "FT".into(),
    };
    if n <= 0.0 {
        return Some("SFC".into());
    }
    if n <= -9990.0 {
        return Some("UNLTD".into());
    }
    let c = match code {
        Value::String(s) => format!(" {}", s.to_uppercase()),
        _ => String::new(),
    };
    Some(if u == "FL" {
        format!("FL{}", n as i64)
    } else {
        format!("{}{c}", n as i64)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn feet_and_labels() {
        assert_eq!(to_feet(&json!("5000"), &json!("FT")), Some(5000.0));
        assert_eq!(to_feet(&json!("180"), &json!("FL")), Some(18000.0));
        assert_eq!(to_feet(&json!(-9998), &json!("FT")), None);
        assert_eq!(label(&json!(0), &json!("FT"), &json!("MSL")).as_deref(), Some("SFC"));
        assert_eq!(label(&json!("100"), &json!("FL"), &json!("STD")).as_deref(), Some("FL100"));
    }

    #[test]
    fn categorizes_class_and_sua() {
        let f = json!({
            "type":"Feature",
            "geometry":{"type":"Polygon","coordinates":[]},
            "properties":{"CLASS":"C","NAME":"X","LOWER_VAL":0,"LOWER_UOM":"FT","UPPER_VAL":4000,"UPPER_UOM":"FT"}
        });
        let n = normalize(f).unwrap();
        assert_eq!(n["properties"]["category"], "CLASS_C");

        let f2 = json!({
            "type":"Feature","geometry":{"type":"Polygon","coordinates":[]},
            "properties":{"TYPE_CODE":"MOA","NAME":"BUCKEYE MOA","LOWER_VAL":"5000","LOWER_UOM":"FT","UPPER_VAL":"180","UPPER_UOM":"FL"}
        });
        let n2 = normalize(f2).unwrap();
        assert_eq!(n2["properties"]["category"], "MOA");
        assert_eq!(n2["properties"]["upperFt"], 18000.0);
    }
}
