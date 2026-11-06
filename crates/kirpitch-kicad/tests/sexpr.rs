// SPDX-License-Identifier: GPL-3.0-or-later

//! The S-expression writer must reproduce KiCad's own layout exactly.

use kirpitch_kicad::sexpr::{Layout, ParseError, RoundTripError, check_round_trip, parse};
use kirpitch_kicad::{List, list};

const TINY_MOD: &str = include_str!("fixtures/tiny.kicad_mod");
const TINY_SYM: &str = include_str!("fixtures/tiny.kicad_sym");

fn font() -> List {
    list("font").child(list("size").num(1.27).num(1.27))
}

fn effects() -> List {
    list("effects").child(font())
}

fn pad(number: &str, x: f64) -> List {
    list("pad")
        .string(number)
        .atom("smd")
        .atom("rect")
        .child(list("at").num(x).num(0.0))
        .child(list("size").num(1.0).num(1.0))
        .child(
            list("layers")
                .string("F.Cu")
                .string("F.Mask")
                .string("F.Paste"),
        )
}

fn tiny_footprint() -> List {
    list("footprint")
        .string("Tiny_2Pad")
        .child(list("version").atom("20260206"))
        .child(list("generator").string("kirpitch-test"))
        .child(list("layer").string("F.Cu"))
        .child(list("attr").atom("smd"))
        .child(pad("1", -1.0))
        .child(pad("2", 1.0))
}

fn property(name: &str, value: &str, at_y: f64, hide: bool) -> List {
    let mut p = list("property")
        .string(name)
        .string(value)
        .child(list("at").num(0.0).num(at_y).num(0.0))
        .child(list("show_name").atom("no"))
        .child(list("do_not_autoplace").atom("no"));
    if hide {
        p = p.child(list("hide").atom("yes"));
    }
    p.child(effects())
}

fn pin_text(kind: &str, text: &str) -> List {
    list(kind).string(text).child(effects())
}

fn tiny_symbol() -> List {
    let body = list("symbol")
        .string("Tiny_1_1")
        .child(
            list("rectangle")
                .child(list("start").num(-2.54).num(1.27))
                .child(list("end").num(2.54).num(-1.27))
                .child(
                    list("stroke")
                        .child(list("width").num(0.254))
                        .child(list("type").atom("default")),
                )
                .child(list("fill").child(list("type").atom("background"))),
        )
        .child(
            list("pin")
                .atom("passive")
                .atom("line")
                .child(list("at").num(-5.08).num(0.0).num(0.0))
                .child(list("length").num(2.54))
                .child(pin_text("name", "A"))
                .child(pin_text("number", "1")),
        );
    let symbol = list("symbol")
        .string("Tiny")
        .child(list("pin_names").child(list("offset").num(0.127)))
        .child(list("exclude_from_sim").atom("no"))
        .child(list("in_bom").atom("yes"))
        .child(list("on_board").atom("yes"))
        .child(list("in_pos_files").atom("yes"))
        .child(list("duplicate_pin_numbers_are_jumpers").atom("no"))
        .child(property("Reference", "U", 2.54, false))
        .child(property("Value", "Tiny", -2.54, false))
        .child(property("Footprint", "", 0.0, true))
        .child(property("Datasheet", "", 0.0, true))
        .child(property("Description", "", 0.0, true))
        .child(body)
        .child(list("embedded_fonts").atom("no"));
    list("kicad_symbol_lib")
        .child(list("version").atom("20251024"))
        .child(list("generator").string("kirpitch-test"))
        .child(list("generator_version").string("10.0"))
        .child(symbol)
}

#[test]
fn footprint_is_written_exactly_like_kicad_would() {
    assert_eq!(tiny_footprint().write(), TINY_MOD);
}

#[test]
fn symbol_is_written_exactly_like_kicad_would() {
    // The fixture is the output of `kicad-cli sym upgrade` (generator renamed),
    // so this proves the layout rules and the default properties.
    assert_eq!(tiny_symbol().write(), TINY_SYM);
}

#[test]
fn fixtures_parse_and_print_back_identically() {
    check_round_trip(TINY_MOD).expect("footprint round trip");
    check_round_trip(TINY_SYM).expect("symbol round trip");
}

#[test]
fn crlf_input_is_accepted_for_the_round_trip() {
    check_round_trip(&TINY_MOD.replace('\n', "\r\n")).expect("crlf round trip");
}

#[test]
fn parsed_tree_equals_built_tree() {
    assert_eq!(parse(TINY_MOD).unwrap(), tiny_footprint());
    assert_eq!(parse(TINY_SYM).unwrap(), tiny_symbol());
}

#[test]
fn symbol_point_lists_stay_on_one_line() {
    let poly = list("polyline").child(
        list("pts")
            .child(list("xy").num(-5.08).num(5.08))
            .child(list("xy").num(5.08).num(0.0))
            .child(list("xy").num(-5.08).num(-5.08))
            .inline_children(),
    );
    let text = poly.write();
    assert_eq!(
        text,
        "(polyline\n\t(pts\n\t\t(xy -5.08 5.08) (xy 5.08 0) (xy -5.08 -5.08)\n\t)\n)\n"
    );
    let parsed = check_round_trip(&text).expect("round trip");
    let Some(kirpitch_kicad::Node::List(pts)) = parsed.items.get(1) else {
        panic!("pts missing");
    };
    assert_eq!(pts.layout, Layout::InlineChildren);
}

#[test]
fn strings_are_escaped_and_read_back() {
    let l = list("property").string("a \"quoted\" \\ name\nnext");
    let text = l.write();
    assert_eq!(text, "(property \"a \\\"quoted\\\" \\\\ name\\nnext\")\n");
    assert_eq!(parse(&text).unwrap(), l);
}

#[test]
fn numbers_follow_the_canonical_format() {
    let l = list("at")
        .num(1.0)
        .num(-0.0)
        .num(0.1 + 0.2)
        .num(1.234_567_89);
    assert_eq!(l.write(), "(at 1 0 0.3 1.234568)\n");
}

#[test]
fn bad_text_is_rejected() {
    assert_eq!(
        parse("(a (b)").unwrap_err(),
        ParseError::Unclosed { line: 1 }
    );
    assert_eq!(parse("(a))").unwrap_err(), ParseError::TopLevel { line: 1 });
    assert_eq!(parse("a").unwrap_err(), ParseError::TopLevel { line: 1 });
    assert_eq!(parse("").unwrap_err(), ParseError::TopLevel { line: 1 });
    assert_eq!(
        parse("(a\n \"open)").unwrap_err(),
        ParseError::UnterminatedString { line: 2 }
    );
    assert_eq!(
        parse("(a) (b)").unwrap_err(),
        ParseError::TopLevel { line: 1 }
    );
}

#[test]
fn non_canonical_layout_fails_the_round_trip() {
    let err = check_round_trip("(a (b 1) (c 2))\n").unwrap_err();
    assert!(matches!(err, RoundTripError::Differs { .. }), "{err}");
    let err = check_round_trip(&TINY_MOD.replace("\t(attr smd)", "    (attr smd)")).unwrap_err();
    assert!(matches!(err, RoundTripError::Differs { line: 5 }), "{err}");
}

#[test]
#[should_panic(expected = "invalid S-expression atom")]
fn atoms_cannot_contain_spaces() {
    let _ = list("a b");
}
