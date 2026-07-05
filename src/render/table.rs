use markdown::mdast::{AlignKind, Node, Table};
use tabled::Table as TabledTable;
use tabled::builder::Builder;
use tabled::settings::object::{Columns, Rows};
use tabled::settings::style::HorizontalLine;
use tabled::settings::{Alignment, Color, Modify, Style};

use super::inline::{emit, spans};

/// Render a GFM table as a bordered `tabled` table. Cells are inline-styled via
/// `emit`; `tabled`'s `ansi` feature keeps columns aligned despite ANSI codes.
pub(crate) fn render_table(table: &Table, color: bool) -> String {
    let mut builder = Builder::default();
    for row in &table.children {
        let Node::TableRow(row) = row else { continue };
        let cells: Vec<String> = row
            .children
            .iter()
            .map(|cell| match cell {
                Node::TableCell(cell) => emit(&spans(&cell.children), color).replace('\n', " "),
                _ => String::new(),
            })
            .collect();
        builder.push_record(cells);
    }
    let mut out = builder.build();
    apply_style(&mut out, &table.align, color);
    out.to_string()
}

fn apply_style(table: &mut TabledTable, align: &[AlignKind], color: bool) {
    let header_rule = HorizontalLine::inherit(Style::extended())
        .remove_intersection()
        .left('╞')
        .right('╡');
    table.with(
        Style::modern()
            .remove_vertical()
            .horizontals([(1, header_rule)]),
    );
    for (index, kind) in align.iter().enumerate() {
        let alignment = match kind {
            AlignKind::Left => Some(Alignment::left()),
            AlignKind::Center => Some(Alignment::center()),
            AlignKind::Right => Some(Alignment::right()),
            AlignKind::None => None,
        };
        if let Some(alignment) = alignment {
            table.with(Modify::new(Columns::one(index)).with(alignment));
        }
    }
    if color {
        table.with(Modify::new(Rows::first()).with(Color::BOLD));
    }
}

#[cfg(test)]
mod tests {
    use crate::render::render;

    const TABLE: &str = "| Name | Qty |\n| :--- | ---: |\n| Apples | 3 |\n| Pears | 12 |\n";

    #[test]
    fn renders_headers_and_cells() {
        let out = render(TABLE, 80, false);
        assert!(out.contains("Name"), "in {out:?}");
        assert!(out.contains("Qty"), "in {out:?}");
        assert!(out.contains("Apples"), "in {out:?}");
        assert!(out.contains("12"), "in {out:?}");
    }

    #[test]
    fn draws_box_borders() {
        let out = render(TABLE, 80, false);
        assert!(
            out.contains('│') || out.contains('─'),
            "expected borders in {out:?}"
        );
    }

    #[test]
    fn bold_header_when_color() {
        let out = render(TABLE, 80, true);
        assert!(out.contains("\x1b[1m"), "expected bold header in {out:?}");
    }
}
