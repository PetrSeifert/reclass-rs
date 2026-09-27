//! The card graph and the per-frame view of it.
//!
//! A card is "the instance of class C at address A". Its links are the pointer
//! rows (`card id` + `row key`) that were followed to reach it. `links[0]` is the
//! anchor: its live value decides the card's address. Further links are aliases
//! that are only drawn as wires; if their pointer moves away they go stale, but
//! they never move or close the card, so pointer churn can't rearrange the canvas.
//! A card whose anchor cannot resolve is re-anchored on an alias that still
//! reaches the same instance, or kept with an error until it recovers or is closed.

use std::collections::{
    BTreeSet,
    HashMap,
};

use reclass_core::{
    decode::{
        category,
        element_field,
        Category,
        Child,
        Decoded,
        Decoder,
    },
    memory::{
        FieldDefinition,
        FieldType,
        PointerTarget,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

pub const ROOT: u64 = 1;
const MAX_DEPTH: u32 = 24;
pub const CARD_WIDTH: f64 = 360.0;
const ROW_HEIGHT: f64 = 24.0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub card: u64,
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub id: u64,
    pub links: Vec<Link>,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub expanded: BTreeSet<String>,
    /// Retain the class label when the anchor is temporarily unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    class_id: Option<u64>,
    /// Instance the card showed last frame; used to pick a replacement anchor.
    #[serde(skip)]
    last: Option<(u64, u64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Canvas {
    pub cards: Vec<Card>,
    pub share: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            cards: vec![Card {
                id: ROOT,
                links: vec![],
                x: 0.0,
                y: 0.0,
                expanded: BTreeSet::new(),
                class_id: None,
                last: None,
            }],
            share: true,
        }
    }
}

/// One visible row of a card.
pub struct RowData {
    pub key: String,
    pub depth: u32,
    pub field: FieldDefinition,
    /// Owning class for editable rows; `None` for array elements.
    pub owner: Option<u64>,
    pub offset: u64,
    pub address: u64,
    pub size: u64,
    pub decoded: Decoded,
    pub open: bool,
}

impl RowData {
    /// `(class, base)` this row's pointer leads to, if it opens cards.
    pub fn target(&self) -> Option<(u64, u64)> {
        match self.decoded.child {
            Some(Child::Class {
                class_id,
                base,
                via_pointer: true,
            }) => Some((class_id, base)),
            _ => None,
        }
    }

    fn has_port(&self) -> bool {
        matches!(
            self.field.field_type,
            FieldType::Pointer | FieldType::EncryptedPointer
        ) && matches!(self.field.pointer_target, Some(PointerTarget::ClassId(_)))
    }
}

pub struct Resolved {
    pub class_id: u64,
    pub error: Option<String>,
    pub base: Option<u64>,
    pub via: Option<String>,
    pub enc: bool,
    pub rows: Vec<RowData>,
    pub dup_of: Option<u64>,
}

/// Rows of an instance, expanding embedded classes and arrays that are open.
pub fn build_rows(
    dec: &Decoder,
    class_id: u64,
    base: u64,
    expanded: &BTreeSet<String>,
) -> Vec<RowData> {
    fn walk(
        dec: &Decoder,
        out: &mut Vec<RowData>,
        fields: Vec<(FieldDefinition, Option<u64>, String)>,
        base: u64,
        depth: u32,
        expanded: &BTreeSet<String>,
    ) {
        let mut offset = 0;
        for (field, owner, key) in fields {
            let size = dec.layout.field_size(&field);
            let address = base.wrapping_add(offset);
            let decoded = dec.decode(&field, address);
            let inline = matches!(
                decoded.child,
                Some(Child::Class {
                    via_pointer: false,
                    ..
                }) | Some(Child::Elements { .. })
            );
            let open = inline && depth < MAX_DEPTH && expanded.contains(&key);
            let child = decoded.child.clone();
            out.push(RowData {
                key: key.clone(),
                depth,
                field,
                owner,
                offset,
                address,
                size,
                decoded,
                open,
            });
            if open {
                match child {
                    Some(Child::Class { class_id, base, .. }) => walk(
                        dec,
                        out,
                        class_fields(dec, class_id, &key),
                        base,
                        depth + 1,
                        expanded,
                    ),
                    Some(Child::Elements {
                        base,
                        element,
                        length,
                    }) => {
                        let elems = (0..length)
                            .map(|i| (element_field(&element, i), None, format!("{key}/#{i}")))
                            .collect();
                        walk(dec, out, elems, base, depth + 1, expanded)
                    }
                    None => {}
                }
            }
            offset += size;
        }
    }
    fn class_fields(
        dec: &Decoder,
        class_id: u64,
        prefix: &str,
    ) -> Vec<(FieldDefinition, Option<u64>, String)> {
        dec.layout
            .classes
            .get(class_id)
            .map(|c| {
                c.fields
                    .iter()
                    .map(|f| (f.clone(), Some(class_id), format!("{prefix}/{}", f.id)))
                    .collect()
            })
            .unwrap_or_default()
    }
    let mut out = Vec::new();
    walk(
        dec,
        &mut out,
        class_fields(dec, class_id, ""),
        base,
        0,
        expanded,
    );
    out
}

impl Canvas {
    pub fn card(&self, id: u64) -> Option<&Card> {
        self.cards.iter().find(|c| c.id == id)
    }

    fn card_mut(&mut self, id: u64) -> Option<&mut Card> {
        self.cards.iter_mut().find(|c| c.id == id)
    }

    fn next_id(&self) -> u64 {
        self.cards.iter().map(|c| c.id).max().unwrap_or(ROOT) + 1
    }

    fn linked_card(&self, link: &Link) -> Option<u64> {
        self.cards
            .iter()
            .find(|c| c.links.contains(link))
            .map(|c| c.id)
    }

    /// Resolves every card against live memory, retaining unresolved cards.
    /// Only cards orphaned by explicit unlinking or closing are removed.
    pub fn resolve(
        &mut self,
        dec: &Decoder,
        root_class: u64,
        root_base: Option<u64>,
    ) -> HashMap<u64, Resolved> {
        let mut done: HashMap<u64, Resolved> = HashMap::new();
        let root_expanded = self
            .card(ROOT)
            .map(|c| c.expanded.clone())
            .unwrap_or_default();
        if let Some(root) = self.card_mut(ROOT) {
            root.last = root_base.map(|b| (root_class, b));
        }
        done.insert(
            ROOT,
            Resolved {
                class_id: root_class,
                error: root_base
                    .is_none()
                    .then(|| "Root address does not resolve".into()),
                base: root_base,
                via: None,
                enc: false,
                rows: root_base
                    .map(|b| build_rows(dec, root_class, b, &root_expanded))
                    .unwrap_or_default(),
                dup_of: None,
            },
        );
        loop {
            let mut progress = false;
            for i in 0..self.cards.len() {
                let card = &self.cards[i];
                if done.contains_key(&card.id) || card.links.is_empty() {
                    continue;
                }
                let anchor = &card.links[0];
                if !done.contains_key(&anchor.card) {
                    continue;
                }
                if let Some((class_id, base, via, enc)) = target_of(&done, anchor) {
                    let rows = build_rows(dec, class_id, base, &card.expanded);
                    let id = card.id;
                    self.cards[i].last = Some((class_id, base));
                    self.cards[i].class_id = Some(class_id);
                    done.insert(
                        id,
                        Resolved {
                            class_id,
                            error: None,
                            base: Some(base),
                            via: Some(via),
                            enc,
                            rows,
                            dup_of: None,
                        },
                    );
                    progress = true;
                }
            }
            if progress {
                continue;
            }
            // Stuck: promote an alias whose source is resolved and still reaches the card.
            for card in self.cards.iter_mut() {
                if done.contains_key(&card.id) {
                    continue;
                }
                let last = card.last;
                let found = card.links.iter().skip(1).position(|l| {
                    done.contains_key(&l.card)
                        && last.is_some()
                        && target_of(&done, l).map(|t| (t.0, t.1)) == last
                });
                if let Some(j) = found {
                    let l = card.links.remove(j + 1);
                    card.links.insert(0, l);
                    progress = true;
                    break;
                }
            }
            if progress {
                continue;
            }
            let dead: Vec<u64> = self
                .cards
                .iter()
                .filter(|c| c.id != ROOT && c.links.is_empty())
                .map(|c| c.id)
                .collect();
            if dead.is_empty() {
                break;
            }
            for id in dead {
                self.remove_card(id, &done);
            }
        }
        // A failed read is not a graph edit. Keep the links, position, expansion
        // state and last instance so the next frame can resolve the card again.
        for card in &self.cards {
            if done.contains_key(&card.id) {
                continue;
            }
            let row = card.links.first().and_then(|link| {
                done.get(&link.card)?
                    .rows
                    .iter()
                    .find(|r| r.key == link.key)
            });
            let error = row
                .and_then(|r| r.decoded.error.clone())
                .unwrap_or_else(|| "Anchor pointer does not resolve".into());
            done.insert(
                card.id,
                Resolved {
                    class_id: card.class_id.or(card.last.map(|last| last.0)).unwrap_or(0),
                    error: Some(error),
                    base: None,
                    via: row.and_then(|r| r.field.name.clone()),
                    enc: row.is_some_and(|r| r.field.field_type == FieldType::EncryptedPointer),
                    rows: vec![],
                    dup_of: None,
                },
            );
        }
        // Two anchors that lead to the same instance: offer a merge (never automatic).
        if self.share {
            let ids: Vec<u64> = self.cards.iter().map(|c| c.id).collect();
            for (i, id) in ids.iter().enumerate() {
                let key = done.get(id).and_then(|r| r.base.map(|b| (r.class_id, b)));
                let dup = ids[..i]
                    .iter()
                    .copied()
                    .find(|o| done.get(o).and_then(|r| r.base.map(|b| (r.class_id, b))) == key);
                if let (Some(_), Some(r)) = (key, done.get_mut(id)) {
                    r.dup_of = dup;
                }
            }
        }
        done
    }

    /// Removes a card and every link that started on it.
    fn remove_card(&mut self, id: u64, resolved: &HashMap<u64, Resolved>) {
        if id == ROOT {
            return;
        }
        self.cards.retain(|c| c.id != id);
        for i in 0..self.cards.len() {
            while let Some(j) = self.cards[i].links.iter().position(|l| l.card == id) {
                self.drop_link(i, j, resolved);
            }
        }
    }

    /// Removes a link. If it was the anchor, an alias that still reaches the same
    /// instance takes over; with none left the card is orphaned (and removed on
    /// the next resolve).
    fn drop_link(
        &mut self,
        card_index: usize,
        link_index: usize,
        resolved: &HashMap<u64, Resolved>,
    ) {
        let card = &mut self.cards[card_index];
        card.links.remove(link_index);
        if link_index == 0 && card.id != ROOT && !card.links.is_empty() {
            let last = card.last;
            match card
                .links
                .iter()
                .position(|l| last.is_some() && target_of(resolved, l).map(|t| (t.0, t.1)) == last)
            {
                Some(j) => {
                    let l = card.links.remove(j);
                    card.links.insert(0, l);
                }
                None => card.links.clear(),
            }
        }
    }

    /// Clicking a pointer port: unlink if linked; otherwise link to the card that
    /// already shows the instance (when sharing), otherwise open a new card.
    /// Returns the card the pointer now leads to.
    pub fn follow(
        &mut self,
        card: u64,
        key: &str,
        resolved: &HashMap<u64, Resolved>,
    ) -> Result<Option<u64>, String> {
        let link = Link {
            card,
            key: key.to_string(),
        };
        let source = resolved.get(&card).ok_or("unknown card")?;
        let row_index = source
            .rows
            .iter()
            .position(|r| r.key == key)
            .ok_or("unknown row")?;
        let target = source.rows[row_index].target();
        if let Some(linked) = self.linked_card(&link) {
            let i = self.cards.iter().position(|c| c.id == linked).unwrap();
            let stale = self.cards[i].last != target;
            let j = self.cards[i].links.iter().position(|l| *l == link).unwrap();
            self.drop_link(i, j, resolved);
            if !stale {
                return Ok(None);
            }
        }
        let Some(target) = target else {
            return Err("pointer is null or invalid".into());
        };
        if self.share {
            if let Some(existing) = self.cards.iter_mut().find(|c| c.last == Some(target)) {
                existing.links.push(link);
                return Ok(Some(existing.id));
            }
        }
        let parent = self.card(card).unwrap();
        let x = parent.x + CARD_WIDTH + 140.0;
        let mut y = parent.y + row_index as f64 * ROW_HEIGHT - 30.0;
        let height = |id: u64| {
            resolved
                .get(&id)
                .map(|r| r.rows.len() as f64 * ROW_HEIGHT + 90.0)
                .unwrap_or(400.0)
        };
        while self.cards.iter().any(|o| {
            (o.x - x).abs() < CARD_WIDTH + 20.0 && y < o.y + height(o.id) + 20.0 && y + 120.0 > o.y
        }) {
            y += 40.0;
        }
        let id = self.next_id();
        self.cards.push(Card {
            id,
            links: vec![link],
            x,
            y,
            expanded: BTreeSet::new(),
            class_id: Some(target.0),
            last: Some(target),
        });
        Ok(Some(id))
    }

    pub fn close(&mut self, id: u64, resolved: &HashMap<u64, Resolved>) -> Result<(), String> {
        if id == ROOT {
            return Err("the root card cannot be closed".into());
        }
        self.card(id).ok_or("unknown card")?;
        self.remove_card(id, resolved);
        Ok(())
    }

    pub fn close_all(&mut self) {
        self.cards.retain(|c| c.id == ROOT);
        if let Some(root) = self.card_mut(ROOT) {
            root.links.clear();
        }
    }

    /// Folds card `id` into the earlier card showing the same instance.
    pub fn merge(&mut self, id: u64, resolved: &HashMap<u64, Resolved>) -> Result<u64, String> {
        let into = resolved
            .get(&id)
            .and_then(|r| r.dup_of)
            .ok_or("card has no duplicate")?;
        let card = self.card(id).cloned().ok_or("unknown card")?;
        let target = self.card_mut(into).unwrap();
        for l in card.links {
            if !target.links.contains(&l) {
                target.links.push(l);
            }
        }
        target.expanded.extend(card.expanded);
        self.cards.retain(|c| c.id != id);
        for c in self.cards.iter_mut() {
            for l in c.links.iter_mut() {
                if l.card == id {
                    l.card = into;
                }
            }
        }
        Ok(into)
    }

    pub fn toggle_expand(&mut self, id: u64, key: &str) -> Result<(), String> {
        let card = self.card_mut(id).ok_or("unknown card")?;
        if !card.expanded.remove(key) {
            card.expanded.insert(key.to_string());
        }
        Ok(())
    }

    pub fn move_card(&mut self, id: u64, x: f64, y: f64) -> Result<(), String> {
        let card = self.card_mut(id).ok_or("unknown card")?;
        card.x = x;
        card.y = y;
        Ok(())
    }

    /// Serializable view of every card for this frame.
    pub fn view(&self, dec: &Decoder, resolved: &HashMap<u64, Resolved>) -> Vec<CardView> {
        let instance = |id: u64| {
            resolved
                .get(&id)
                .and_then(|r| r.base.map(|b| (r.class_id, b)))
        };
        self.cards
            .iter()
            .filter_map(|card| {
                let r = resolved.get(&card.id)?;
                let rows = r
                    .rows
                    .iter()
                    .map(|row| {
                        let port = row.has_port().then(|| {
                            let link = Link {
                                card: card.id,
                                key: row.key.clone(),
                            };
                            let target = row.target();
                            let state = match (target, self.linked_card(&link)) {
                                (None, _) => PortState::Null,
                                (Some(t), Some(linked)) if instance(linked) == Some(t) => {
                                    PortState::Open
                                }
                                (Some(_), Some(_)) => PortState::Stale,
                                (Some(t), None)
                                    if self.share
                                        && self.cards.iter().any(|c| instance(c.id) == Some(t)) =>
                                {
                                    PortState::Known
                                }
                                _ => PortState::Closed,
                            };
                            Port {
                                state,
                                encrypted: row.field.field_type == FieldType::EncryptedPointer,
                            }
                        });
                        row_view(dec, row, port)
                    })
                    .collect();
                let links = card
                    .links
                    .iter()
                    .enumerate()
                    .map(|(i, l)| {
                        let t = target_of(resolved, l);
                        LinkView {
                            card: l.card,
                            key: l.key.clone(),
                            anchor: card.id != ROOT && i == 0,
                            ok: t
                                .as_ref()
                                .is_some_and(|t| Some((t.0, t.1)) == instance(card.id)),
                            label: t.as_ref().map(|t| t.2.clone()).unwrap_or_default(),
                            encrypted: t.map(|t| t.3).unwrap_or(false),
                            now: resolved
                                .get(&l.card)
                                .and_then(|s| s.rows.iter().find(|r| r.key == l.key))
                                .and_then(|r| r.decoded.pointer)
                                .map(hex),
                        }
                    })
                    .collect();
                Some(CardView {
                    id: card.id,
                    class_id: r.class_id,
                    class_name: dec
                        .layout
                        .classes
                        .get(r.class_id)
                        .map(|c| c.name.clone())
                        .unwrap_or_default(),
                    size: dec.layout.class_size(r.class_id),
                    base: r.base.map(hex),
                    error: r.error.clone(),
                    via: r.via.clone(),
                    encrypted: r.enc,
                    is_root: card.id == ROOT,
                    x: card.x,
                    y: card.y,
                    dup_of: r.dup_of,
                    links,
                    rows,
                })
            })
            .collect()
    }
}

/// Instance a link currently leads to: `(class, base, row name, encrypted)`.
fn target_of(resolved: &HashMap<u64, Resolved>, link: &Link) -> Option<(u64, u64, String, bool)> {
    let row = resolved
        .get(&link.card)?
        .rows
        .iter()
        .find(|r| r.key == link.key)?;
    let (class_id, base) = row.target()?;
    Some((
        class_id,
        base,
        row.field.name.clone().unwrap_or_default(),
        row.field.field_type == FieldType::EncryptedPointer,
    ))
}

pub fn hex(v: u64) -> String {
    format!("{v:X}")
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PortState {
    /// Not followed yet.
    Closed,
    /// Linked and the pointer still leads to the linked card.
    Open,
    /// Linked, but the pointer moved elsewhere.
    Stale,
    /// Not followed, but the target is already on the canvas.
    Known,
    /// Null or invalid pointer.
    Null,
}

#[derive(Serialize)]
pub struct Port {
    pub state: PortState,
    pub encrypted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowView {
    pub key: String,
    pub depth: u32,
    pub field_id: u64,
    pub class_id: Option<u64>,
    pub offset: u64,
    pub address: String,
    pub size: u64,
    pub name: Option<String>,
    pub ty: FieldType,
    pub cat: Category,
    pub type_label: String,
    pub value: String,
    pub hints: Vec<String>,
    pub error: Option<String>,
    pub bytes: String,
    pub pointer: Option<String>,
    pub expandable: bool,
    pub open: bool,
    pub port: Option<Port>,
    pub pointer_target: Option<PointerTarget>,
    pub enum_id: Option<u64>,
    pub embedded_class: Option<u64>,
    pub array_length: Option<u32>,
    pub array_element: Option<PointerTarget>,
}

fn row_view(dec: &Decoder, row: &RowData, port: Option<Port>) -> RowView {
    let d = &row.decoded;
    RowView {
        key: row.key.clone(),
        depth: row.depth,
        field_id: row.field.id,
        class_id: row.owner,
        offset: row.offset,
        address: hex(row.address),
        size: row.size,
        name: row.field.name.clone(),
        ty: row.field.field_type.clone(),
        cat: category(&row.field.field_type),
        type_label: dec.type_label(&row.field),
        value: d.value.clone(),
        hints: d.hints.clone(),
        error: d.error.clone(),
        bytes: d.bytes.iter().map(|b| format!("{b:02X}")).collect(),
        pointer: d.pointer.map(hex),
        expandable: matches!(
            d.child,
            Some(Child::Class {
                via_pointer: false,
                ..
            }) | Some(Child::Elements { .. })
        ),
        open: row.open,
        port,
        pointer_target: row.field.pointer_target.clone(),
        enum_id: row.field.enum_id,
        embedded_class: row.field.class_id,
        array_length: row.field.array_length,
        array_element: row.field.array_element.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkView {
    pub card: u64,
    pub key: String,
    pub anchor: bool,
    pub ok: bool,
    pub label: String,
    pub encrypted: bool,
    /// Where the pointer currently points (for stale links).
    pub now: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardView {
    pub id: u64,
    pub class_id: u64,
    pub class_name: String,
    pub size: u64,
    pub base: Option<String>,
    pub error: Option<String>,
    pub via: Option<String>,
    pub encrypted: bool,
    pub is_root: bool,
    pub x: f64,
    pub y: f64,
    pub dup_of: Option<u64>,
    pub links: Vec<LinkView>,
    pub rows: Vec<RowView>,
}
