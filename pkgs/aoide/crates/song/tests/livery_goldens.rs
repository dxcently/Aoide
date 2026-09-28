//! Golden-parity integration tests for the livery engine (LIVERY-MERGE.md
//! Phase 1) — the native Rust pipeline asserted against bytes captured from
//! the Node engine BEFORE it was cut: `tests/goldens/<fixture>.<target>.golden`
//! (LIVERY-MERGE Step 1.0).
//!
//! Parity contract (plan §1.3):
//! * `resolve`   — byte-identical (`JSON.stringify(…, null, 2)`, insertion
//!   order; proves the native deref matches Style Dictionary on the
//!   ref-using fixture),
//! * `emit osc`  — byte-identical (OSC streams are order- and byte-sensitive),
//! * `emit hyprctl` — byte-identical (shell-quoted keyword lines),
//! * `emit stage`   — SEMANTIC JSON equality (serde's key order legitimately
//!   differs from `JSON.stringify`; QML reads by key, so shape is the
//!   contract, bytes are not).
//!
//! A fourth fixture, `valid-polarity.json`, is NOT part of that parity set:
//! it pins the NATIVE engine's own posture for a field the Node engine never
//! had — §1's `polarity`, which lint validates and no backend emits — against
//! `valid`'s goldens (`polarity_is_lint_only_and_moves_no_emitted_byte`).

use aoide_song::livery::emit::hyprctl::render_lines;
use aoide_song::livery::emit::{EmitOpts, EmitOutput, emitter};
use aoide_song::livery::resolve::to_json_string;
use aoide_song::livery::{lint, resolve};
use serde_json::Value;

const FIXTURES: [&str; 3] = ["valid", "valid-hot", "valid-base16"];

fn fixture(name: &str) -> Value {
    let raw = std::fs::read_to_string(format!("tests/fixtures/{name}.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(format!("tests/goldens/{name}.golden")).unwrap()
}

/// Resolve + validate, the CLI's exact pipeline order.
fn resolved(fix: &str) -> aoide_song::livery::Resolved {
    let container = fixture(fix);
    assert!(lint(&container).ok, "{fix}: fixture must validate");
    resolve(&container).expect("fixture resolves")
}

#[test]
fn resolve_output_is_byte_identical_to_the_node_goldens() {
    for fix in FIXTURES {
        let r = resolved(fix);
        assert_eq!(
            format!("{}\n", to_json_string(&r)),
            golden(&format!("{fix}.resolve")),
            "{fix}: resolve must be byte-identical to the Node engine"
        );
    }
}

#[test]
fn emit_osc_is_byte_identical_to_the_node_goldens() {
    for fix in FIXTURES {
        let out = emitter("osc")
            .expect("osc backend registered")
            .emit(&resolved(fix), &EmitOpts::default())
            .unwrap();
        match out {
            EmitOutput::Text(s) => {
                assert_eq!(
                    s,
                    golden(&format!("{fix}.emit-osc")),
                    "{fix}: osc stream must be byte-identical to the Node engine"
                );
            }
            other => panic!("{fix}: osc emitted {other:?}, expected Text"),
        }
    }
}

#[test]
fn emit_hyprctl_is_byte_identical_to_the_node_goldens() {
    for fix in FIXTURES {
        let out = emitter("hyprctl")
            .expect("hyprctl backend registered")
            .emit(&resolved(fix), &EmitOpts::default())
            .unwrap();
        match out {
            EmitOutput::Lines(cmds) => {
                assert_eq!(
                    render_lines(&cmds),
                    golden(&format!("{fix}.emit-hyprctl")),
                    "{fix}: hyprctl lines must be byte-identical to the Node engine"
                );
            }
            other => panic!("{fix}: hyprctl emitted {other:?}, expected Lines"),
        }
    }
}

#[test]
fn emit_stage_is_semantically_equal_to_the_node_goldens() {
    for fix in FIXTURES {
        let out = emitter("stage")
            .expect("stage backend registered")
            .emit(&resolved(fix), &EmitOpts::default())
            .unwrap();
        match out {
            EmitOutput::Json(v) => {
                let expected: Value = serde_json::from_str(&golden(&format!("{fix}.emit-stage")))
                    .expect("golden parses");
                assert_eq!(
                    v, expected,
                    "{fix}: stage must be shape-equal to the Node engine (key order may differ)"
                );
            }
            other => panic!("{fix}: stage emitted {other:?}, expected Json"),
        }
    }
}

#[test]
fn file_backend_renders_from_the_registry_with_a_template() {
    let out = emitter("file")
        .expect("file backend registered")
        .emit(
            &resolved("valid"),
            &EmitOpts {
                template: Some("bg={{palette.bg}} border={{window.border}}"),
            },
        )
        .unwrap();
    match out {
        EmitOutput::Text(s) => {
            assert_eq!(s, "bg=#1e1e2e border=#89b4fa");
        }
        other => panic!("file emitted {other:?}, expected Text"),
    }
}

#[test]
fn file_backend_errors_on_an_unknown_placeholder() {
    let err = emitter("file")
        .expect("file backend registered")
        .emit(
            &resolved("valid"),
            &EmitOpts {
                template: Some("{{bogus.bg}}"),
            },
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("unknown placeholder"),
        "structured error, not a panic: {err}"
    );
}

#[test]
fn registry_lists_exactly_the_five_backends_in_order() {
    let targets: Vec<&str> = aoide_song::livery::emit::registry()
        .iter()
        .map(|e| e.target())
        .collect();
    assert_eq!(targets, ["stage", "hyprctl", "osc", "file", "kitty"]);
}

/// The polarity field (CONTRACTS.md §1) is LINT-ONLY: a notes document may
/// carry it, `livery lint` validates it (exactly `"light"` or `"dark"`), and
/// NO backend emits it — it is the baked fan-out's register, which the stylix
/// lane reads off the option, never off a stage file. So a polarity-carrying
/// document must produce byte-for-byte what the same document without the
/// field produces, and the goldens above need no new file for it.
///
/// `valid-polarity.json` is `valid.json` plus `"polarity": "dark"`; the field
/// is asserted present, so the test fails loudly if the fixture ever loses it.
#[test]
fn polarity_is_lint_only_and_moves_no_emitted_byte() {
    let with_polarity = fixture("valid-polarity");
    assert_eq!(
        with_polarity.get("polarity").and_then(Value::as_str),
        Some("dark"),
        "valid-polarity.json must carry the field this test is about"
    );
    let without = fixture("valid");

    // The resolver's canonical output, byte for byte — including its
    // `schemaVersion`+`palette`+… insertion order, which `valid.resolve.golden`
    // pins, so the field is proved absent from the emitted shape.
    assert_eq!(
        format!("{}\n", to_json_string(&resolved("valid-polarity"))),
        golden("valid.resolve"),
        "polarity must not reach `livery resolve`"
    );

    for target in ["osc", "hyprctl", "stage"] {
        let one = emitter(target)
            .expect("backend registered")
            .emit(&resolved("valid-polarity"), &EmitOpts::default())
            .unwrap();
        let two = emitter(target)
            .expect("backend registered")
            .emit(&resolve(&without).expect("fixture resolves"), &EmitOpts::default())
            .unwrap();
        assert_eq!(one, two, "{target}: polarity must not move an emitted byte");
    }
}
