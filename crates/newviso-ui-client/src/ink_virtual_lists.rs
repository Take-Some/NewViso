//! Provider-neutral virtualization of source Ink dynamic list widgets.
//!
//! Original `inkVirtualCompoundWidget` is a *container*, not an item list.
//! An actual game-state adapter supplies rows and this module emits only
//! visible rows. It never invents missing items or saves.
//!
//! All nodes use the ordinary engine.ui schema; UI providers contain no
//! inventory, mouse or controller gameplay semantics.
use serde_json::{json, Value};

pub const INK_ROW_HEIGHT: f64 = 51.0;
const MAX_VISIBLE_ROWS: usize = 32;
const MAX_TOTAL_ROWS: usize = 8192;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkListKind { Inventory, Backpack }

impl InkListKind {
    pub fn from_screen(name: &str) -> Option<Self> {
        match name {
            "inventory_screen" => Some(Self::Inventory),
            "backpack" => Some(Self::Backpack),
            _ => None,
        }
    }
    fn screen(self) -> &'static str {
        match self { Self::Inventory => "inventory_screen", Self::Backpack => "backpack" }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InkVirtualListState {
    pub first_row: usize,
    pub selected_item: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InkVirtualListMetrics {
    pub total_rows: usize,
    pub first_row: usize,
    pub rendered_rows: usize,
    pub selected_item: Option<String>,
}

impl InkVirtualListState {
    pub fn scroll_pages(&mut self, delta: i64, total: usize, visible: usize) {
        let visible = visible.max(1);
        let last = total.saturating_sub(visible);
        let amount = (delta.unsigned_abs() as usize).saturating_mul(visible);
        self.first_row = if delta < 0 {
            self.first_row.saturating_sub(amount)
        } else {
            self.first_row.saturating_add(amount).min(last)
        };
    }
}

/// Draw source Ink virtual list into its original container rectangle.
/// Source widget paths and anchors remain intact; only the dynamically
/// generated row nodes are added. All inputs are already engine-authorized.
pub fn compose_inventory_list(
    source: &mut Value,
    model: &Value,
    state: &mut InkVirtualListState,
    kind: InkListKind,
) -> Result<InkVirtualListMetrics, String> {
    let screen = kind.screen();
    let list = model.pointer(&format!("/screens/{screen}/entries"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Ink {screen} item view model missing"))?;
    let total = list.len().min(MAX_TOTAL_ROWS);
    let components = source.get_mut("components")
        .and_then(Value::as_array_mut)
        .ok_or("Ink source has no components")?;
    let container = components.iter_mut()
        .filter(|node| node.pointer("/props/ink_source_class")
            .and_then(Value::as_str)==Some("inkVirtualCompoundWidget"))
        .filter(|node| node.pointer("/props/w_px").and_then(Value::as_f64).unwrap_or(0.0)>=150.)
        .max_by(|a,b| {
            a.pointer("/props/w_px").and_then(Value::as_f64).unwrap_or(0.)
                .total_cmp(&b.pointer("/props/w_px").and_then(Value::as_f64).unwrap_or(0.))
        })
        .ok_or("Ink source has no measurable virtual compound container")?;
    let rect = &container["props"];
    let width = rect["w_px"].as_f64().unwrap_or(0.);
    let height = rect["h_px"].as_f64().unwrap_or(0.);
    if !width.is_finite() || !height.is_finite() || width<150. || height<51. {
        return Err("Ink virtual container has invalid dimensions".into());
    }
    // Reserve source-space footer for page controls and explicit equipment
    // actions so neither can overlap the virtualized item rows.
    let visible = (((height-84.)/INK_ROW_HEIGHT).floor() as usize)
        .clamp(1,MAX_VISIBLE_ROWS);
    let last = total.saturating_sub(visible);
    state.first_row=state.first_row.min(last);
    if state.selected_item.as_ref().is_some_and(|id| {
        !list.iter().take(total).any(|entry| entry["id"].as_str()==Some(id))
    }) {
        state.selected_item=None;
    }

    let row_width=(width-28.).max(80.);
    let mut children=Vec::with_capacity(visible+3);
    for (row_idx,entry) in list.iter().take(total)
        .enumerate().skip(state.first_row).take(visible)
    {
        let id=entry["id"].as_str().ok_or("Ink inventory item has no id")?;
        let qty=entry["quantity"].as_u64().unwrap_or(0);
        let name=entry["name"].as_str().unwrap_or(id);
        let selected=state.selected_item.as_deref()==Some(id);
        children.push(json!({
            "id":format!("ink.virtual.{screen}.row.{row_idx}"),
            "component_id":"button",
            "text":format!("{name}    ×{qty}"),
            "action_id":format!("newviso.ink.item.select.{row_idx}"),
            "props":{
                "position":"absolute","x_px":8,
                "y_px":8.0+((row_idx-state.first_row) as f64)*INK_ROW_HEIGHT,
                "w_px":row_width,"h_px":INK_ROW_HEIGHT-4.,
                "draw_panel":true,
                "font_px":22,
                "text_rgba":if selected {vec![255,236,120,255]} else {vec![243,247,250,255]},
                "fill_rgba":if selected {vec![39,42,49,232]} else {vec![11,21,31,205]},
                "border_rgba":if selected {vec![255,231,88,255]} else {vec![67,156,170,110]},
                "border_px":1,
                "source_ink_controller":"gameuiInventoryGameController",
                "source_ink_owner":"player"
            }
        }));
    }
    if total==0 {
        children.push(json!({
            "id":format!("ink.virtual.{screen}.empty"),
            "component_id":"text","text":"ИНВЕНТАРЬ ПУСТ",
            "props":{"position":"absolute","x_px":15,"y_px":16,
                "w_px":row_width,"h_px":40,"font_px":21}
        }));
    }
    let nav_y=(height-32.).max(0.);
    if state.first_row>0 {
        children.push(json!({
            "id":format!("ink.virtual.{screen}.previous"),
            "component_id":"button","text":"▲ ПРЕДЫДУЩИЕ",
            "action_id":"newviso.ink.list.previous",
            "props":{"position":"absolute","x_px":8,"y_px":nav_y,
                "w_px":145,"h_px":27,"font_px":14}
        }));
    }
    if state.first_row<last {
        children.push(json!({
            "id":format!("ink.virtual.{screen}.next"),
            "component_id":"button","text":"СЛЕДУЮЩИЕ ▼",
            "action_id":"newviso.ink.list.next",
            "props":{"position":"absolute","x_px":width-160.,"y_px":nav_y,
                "w_px":145,"h_px":27,"font_px":14}
        }));
    }
    if let Some(selected) = state.selected_item.as_deref() {
        let selected_row = list.iter().take(total)
            .find(|entry|entry["id"].as_str()==Some(selected));
        if let Some(row) = selected_row {
            if row["category"].as_str()==Some("weapon") {
                children.push(json!({
                    "id":format!("ink.virtual.{screen}.equip"),
                    "component_id":"button","text":"ЭКИПИРОВАТЬ",
                    "action_id":"newviso.ink.item.equip.hands",
                    "props":{"position":"absolute","x_px":8,
                        "y_px":(height-67.).max(0.),"w_px":220,"h_px":31,
                        "font_px":16,"draw_panel":true,
                        "fill_rgba":[23,44,55,221],
                        "border_rgba":[255,231,88,255],"border_px":1}
                }));
            }
        }
    }
    container["component_id"]=json!("panel");
    container["props"]["draw_panel"]=json!(false);
    container["props"]["ink_virtualized"]=json!(true);
    container["props"]["ink_visible_rows"]=json!(visible);
    container["props"]["ink_total_rows"]=json!(total);
    container["props"]["ink_list_first_row"]=json!(state.first_row);
    container["children"]=Value::Array(children);
    source["metrics"]["ink_dynamic_controller"]=json!("gameuiInventoryGameController");
    Ok(InkVirtualListMetrics {
        total_rows:total,first_row:state.first_row,
        rendered_rows:visible.min(total.saturating_sub(state.first_row)),
        selected_item:state.selected_item.clone(),
    })
}

/// Return an item identity from the current *authoritative* model, never
/// an ID embedded in a received action. Stale/out-of-range indices fail.
pub fn selected_item_from_action<'a>(
    action: &str, model: &'a Value, kind: InkListKind
) -> Result<Option<&'a str>, String> {
    let Some(index)=action.strip_prefix("newviso.ink.item.select.") else {
        return Ok(None);
    };
    if index.is_empty() || index.len()>5 || !index.bytes().all(|c|c.is_ascii_digit()) {
        return Err("invalid Ink inventory selection index".into());
    }
    let index=index.parse::<usize>().map_err(|_|"invalid item index")?;
    if index>=MAX_TOTAL_ROWS {return Err("Ink item index outside bounded view".into());}
    let entries=model.pointer(&format!("/screens/{}/entries",kind.screen()))
        .and_then(Value::as_array)
        .ok_or("inventory source view missing")?;
    let item=entries.get(index).and_then(|row|row["id"].as_str())
        .ok_or("Ink item selection no longer exists")?;
    Ok(Some(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn src() -> Value {
        json!({"components":[{"id":"source.virtual",
            "component_id":"panel","props":{
                "ink_source_class":"inkVirtualCompoundWidget",
                "position":"absolute","x_px":100,"y_px":200,
                "w_px":500,"h_px":300}}],"metrics":{}})
    }
    fn model(count: usize)->Value {
        let entries=(0..count).map(|i|json!({
            "id":format!("weapon.{i}"),
            "name":format!("Weapon {i}"),
            "quantity":i+1
        })).collect::<Vec<_>>();
        json!({"screens":{"inventory_screen":{"entries":entries},
             "backpack":{"entries":[]}}})
    }
    #[test]
    fn composes_real_rows_into_original_virtual_container() {
        let mut surface=src();
        let mut list=InkVirtualListState::default();
        let metrics=compose_inventory_list(&mut surface,&model(20),&mut list,
            InkListKind::Inventory).unwrap();
        assert_eq!(metrics.total_rows,20);
        assert_eq!(metrics.rendered_rows,4);
        assert_eq!(surface["components"][0]["props"]["x_px"],100);
        assert_eq!(surface["components"][0]["props"]["y_px"],200);
        assert_eq!(surface["components"][0]["props"]["ink_virtualized"],true);
        assert_eq!(surface["components"][0]["children"][0]["text"],"Weapon 0    ×1");
        assert_eq!(surface["components"][0]["children"][0]["action_id"],
                   "newviso.ink.item.select.0");
    }
    #[test]
    fn paging_clamps_to_end_and_preserves_item_selection() {
        let mut surface=src();
        let mut state=InkVirtualListState::default();
        state.scroll_pages(100,50,5);
        assert_eq!(state.first_row,45);
        state.selected_item=Some("weapon.48".into());
        let metrics=compose_inventory_list(&mut surface,&model(50),&mut state,
            InkListKind::Inventory).unwrap();
        assert_eq!(metrics.first_row,45);
        assert_eq!(metrics.rendered_rows,4);
        assert_eq!(metrics.selected_item.as_deref(),Some("weapon.48"));
        state.scroll_pages(-100,50,5);
        assert_eq!(state.first_row,0);
    }
    #[test]
    fn stale_selection_is_cleared_after_inventory_mutation() {
        let mut state=InkVirtualListState {
            selected_item:Some("removed".into()),first_row:100
        };
        compose_inventory_list(&mut src(),&model(2),&mut state,
            InkListKind::Inventory).unwrap();
        assert_eq!(state.first_row,0);
        assert!(state.selected_item.is_none());
    }
    #[test]
    fn action_selection_is_bound_to_current_model_and_bounded() {
        let snapshot=model(4);
        assert_eq!(selected_item_from_action("newviso.ink.item.select.2",&snapshot,
            InkListKind::Inventory).unwrap(),Some("weapon.2"));
        assert!(selected_item_from_action("newviso.ink.item.select.4",&snapshot,
            InkListKind::Inventory).is_err());
        assert!(selected_item_from_action("newviso.ink.item.select.-1",&snapshot,
            InkListKind::Inventory).is_err());
        assert_eq!(selected_item_from_action("menu.quit",&snapshot,
            InkListKind::Inventory).unwrap(),None);
    }
    #[test]
    fn original_inventory_surface_accepts_live_item_rows() {
        let Ok(path)=std::env::var("NEWVISO_INK_INVENTORY_SURFACE_TEST_PATH") else {
            return;
        };
        let mut surface: Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut state=InkVirtualListState::default();
        let metrics=compose_inventory_list(&mut surface,&model(300),&mut state,
            InkListKind::Inventory).expect("original Ink virtual list binding");
        assert_eq!(metrics.total_rows,300);
        assert!(metrics.rendered_rows>0 && metrics.rendered_rows<=MAX_VISIBLE_ROWS);
        let target=surface["components"].as_array().unwrap().iter()
            .find(|node|node["props"]["ink_virtualized"]==true).unwrap();
        assert_eq!(target["props"]["ink_source_class"],"inkVirtualCompoundWidget");
        assert!(target["children"].as_array().unwrap().len()<=MAX_VISIBLE_ROWS+3);
    }

    #[test]
    fn empty_data_model_never_fabricates_items() {
        let mut surface=src();
        let mut state=InkVirtualListState::default();
        let metrics=compose_inventory_list(&mut surface,&model(0),&mut state,
            InkListKind::Inventory).unwrap();
        assert_eq!(metrics.rendered_rows,0);
        assert_eq!(surface["components"][0]["children"][0]["text"],"ИНВЕНТАРЬ ПУСТ");
    }
}
