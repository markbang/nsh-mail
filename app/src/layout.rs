//! HTML mail as `TextView` reads it. Mail lays out with tables, and `TextView` shows a table
//! as a grid of small text whose cells hold one line each: headings, lists and paragraphs inside
//! a layout table would run together. Layout tables become blocks here, before `TextView`
//! parses the HTML; tables of data stay tables.

use std::cell::RefCell;

use html5ever::serialize::{SerializeOpts, TraversalScope, serialize};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{LocalName, Namespace, QualName};
use markup5ever_rcdom::{Handle, Node, NodeData, RcDom, SerializableHandle};

/// Elements that start a block: a table cell holding one lays out content, not data.
const BLOCKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];
/// Nesting deeper than this is left as it is: hostile mail may nest without bound.
const MAX_DEPTH: usize = 200;
/// Between the cells of a row read as one line: an em space, which HTML does not collapse.
const CELL_GAP: &str = "\u{2003}";

/// `html` with its layout tables as blocks: one per row, the short cells of a row on one line,
/// so what is inside stays what it is and no cell's `align` or size reaches it. Tables of data
/// (a header row of `<th>`, cells of plain text) stay tables.
pub fn reading_html(html: &str) -> String {
    if !html.to_ascii_lowercase().contains("<table") {
        return html.to_string();
    }
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let Some(body) = child(&dom.document, "html").and_then(|h| child(&h, "body")) else {
        return html.to_string();
    };
    rewrite(&body, 0);
    let mut out = Vec::new();
    let opts = SerializeOpts {
        traversal_scope: TraversalScope::ChildrenOnly(None),
        ..Default::default()
    };
    match serialize(&mut out, &SerializableHandle::from(body), opts) {
        Ok(()) => String::from_utf8(out).unwrap_or_else(|_| html.to_string()),
        Err(_) => html.to_string(),
    }
}

fn rewrite(node: &Handle, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let children = std::mem::take(&mut *node.children.borrow_mut());
    let mut out = Vec::with_capacity(children.len());
    for c in children {
        if name(&c) == Some("table") && !is_data_table(&c) {
            if !is_hidden(&c) {
                out.push(element("div", unwrap_table(&c, depth + 1)));
            }
        } else {
            rewrite(&c, depth + 1);
            out.push(c);
        }
    }
    *node.children.borrow_mut() = out;
}

fn unwrap_table(table: &Handle, depth: usize) -> Vec<Handle> {
    let mut blocks = Vec::new();
    for part in kids(table) {
        if name(&part) == Some("caption") {
            rewrite(&part, depth + 1);
            blocks.push(element("div", take(&part)));
        }
    }
    // Rows of text are lines of one paragraph; a row holding blocks is blocks of its own.
    let mut lines = Vec::new();
    for row in rows(table).into_iter().filter(|r| !is_hidden(r)) {
        let cells: Vec<Handle> = kids(&row)
            .into_iter()
            .filter(|c| matches!(name(c), Some("td" | "th")) && !is_blank(c) && !is_hidden(c))
            .collect();
        if cells.is_empty() {
            continue;
        }
        let stacked = cells.iter().any(has_block);
        cells.iter().for_each(|c| rewrite(c, depth + 1));
        if stacked {
            if !lines.is_empty() {
                blocks.push(element("div", std::mem::take(&mut lines)));
            }
            let parts = match cells.as_slice() {
                [cell] => take(cell),
                _ => cells.iter().map(|c| element("div", take(c))).collect(),
            };
            blocks.push(element("div", parts));
            continue;
        }
        if !lines.is_empty() {
            lines.push(element("br", Vec::new()));
        }
        for (i, c) in cells.iter().enumerate() {
            if i > 0 {
                lines.push(Node::new(NodeData::Text {
                    contents: RefCell::new(StrTendril::from(CELL_GAP)),
                }));
            }
            let tag = if name(c) == Some("th") { "b" } else { "span" };
            lines.push(element(tag, take(c)));
        }
    }
    if !lines.is_empty() {
        blocks.push(element("div", lines));
    }
    blocks
}

fn is_data_table(table: &Handle) -> bool {
    let rows = rows(table);
    let head = rows.first().map(kids).unwrap_or_default();
    let head: Vec<_> = head
        .iter()
        .filter(|c| matches!(name(c), Some("td" | "th")))
        .collect();
    rows.len() >= 2
        && head.len() >= 2
        && head.iter().all(|c| name(c) == Some("th"))
        && rows.iter().all(|r| !kids(r).iter().any(has_block))
}

fn rows(table: &Handle) -> Vec<Handle> {
    let mut rows = Vec::new();
    for part in kids(table) {
        match name(&part) {
            Some("tr") => rows.push(part),
            Some("thead" | "tbody" | "tfoot") => {
                rows.extend(kids(&part).into_iter().filter(|r| name(r) == Some("tr")))
            }
            _ => {}
        }
    }
    rows
}

fn has_block(node: &Handle) -> bool {
    descendants(node).any(|n| name(&n).is_some_and(|n| BLOCKS.contains(&n)))
}

/// Spacer cells (`&nbsp;`, nothing): a reader shows no empty lines for them.
fn is_blank(node: &Handle) -> bool {
    descendants(node).all(|n| match &n.data {
        NodeData::Text { contents } => contents
            .borrow()
            .chars()
            .all(|c| c.is_whitespace() || c == '\u{200b}' || c == '\u{feff}'),
        NodeData::Element { name, .. } => !matches!(&*name.local, "img" | "hr" | "svg" | "video"),
        _ => true,
    })
}

/// Hidden as `TextView` reads it; the attributes that say so go with the table's elements.
fn is_hidden(node: &Handle) -> bool {
    let NodeData::Element { attrs, .. } = &node.data else {
        return false;
    };
    attrs.borrow().iter().any(|a| match &*a.name.local {
        "hidden" => true,
        "style" => a.value.to_ascii_lowercase().split(';').any(|d| {
            d.split_once(':')
                .is_some_and(|(k, v)| k.trim() == "display" && v.trim().starts_with("none"))
        }),
        _ => false,
    })
}

fn descendants(node: &Handle) -> impl Iterator<Item = Handle> {
    let mut stack = kids(node);
    std::iter::from_fn(move || {
        let n = stack.pop()?;
        stack.extend(kids(&n));
        Some(n)
    })
}

/// The children of `node`, moved out: dropping a node empties every node below it, those
/// still referenced elsewhere included (rcdom drops without recursion).
fn take(node: &Handle) -> Vec<Handle> {
    std::mem::take(&mut *node.children.borrow_mut())
}

fn kids(node: &Handle) -> Vec<Handle> {
    node.children.borrow().clone()
}

fn child(node: &Handle, tag: &str) -> Option<Handle> {
    kids(node).into_iter().find(|c| name(c) == Some(tag))
}

fn name(node: &Handle) -> Option<&str> {
    match &node.data {
        NodeData::Element { name, .. } => Some(&name.local),
        _ => None,
    }
}

fn element(tag: &str, children: Vec<Handle>) -> Handle {
    let node = Node::new(NodeData::Element {
        name: QualName::new(
            None,
            Namespace::from("http://www.w3.org/1999/xhtml"),
            LocalName::from(tag),
        ),
        attrs: RefCell::new(Vec::new()),
        template_contents: RefCell::new(None),
        mathml_annotation_xml_integration_point: false,
    });
    *node.children.borrow_mut() = children;
    node
}
