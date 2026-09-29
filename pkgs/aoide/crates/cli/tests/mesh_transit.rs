//! The five-edge fixture mesh (see `common/mod.rs`), loaded and validated
//! through the one declaration seam, plus its two deliberately broken
//! charters and the route it declares. No child process and no network: this
//! is the fixture proving ITSELF — that what it declares is what the four
//! steps read.

mod common;

use common::{fixture, env_lock, AWAY, HOME, ONE_SIDED_GATE, TWO_KEYS};

use aoide_storage::routing;

#[test]
fn the_five_edge_fixture_loads_and_validates_through_the_declaration_seam() {
    let _lock = env_lock();
    let (_env, fx) = fixture("load");
    // `sakaki` is the box in BOTH meshes, so it is the one that reads a set
    // with two declarations in it.
    fx.enter("sakaki");

    let set = routing::declarations();
    for loaded in &set {
        assert!(loaded.declaration.is_ok(), "`{}` loads: {:?}", loaded.mesh, loaded.declaration);
    }
    let declaration = |mesh: &str| {
        set.iter()
            .find(|l| l.mesh == mesh)
            .and_then(|l| l.declaration.as_ref().ok())
            .unwrap_or_else(|| panic!("`{mesh}` is declared here: {set:?}"))
    };
    assert_eq!(set.len(), 2, "both meshes are declared here: {set:?}");
    let home = declaration(HOME);
    let away = declaration(AWAY);

    assert!(home.is_charter() && away.is_charter(), "both are charter meshes here");
    assert_eq!(home.relays(), ["sakaki".to_string()], "the relay, in declaration order");
    assert_eq!(home.status_of("yomi"), Some("down"));
    assert_eq!(home.status_of("chiyo"), None, "a declared node with no status is not an undeclared one");
    assert_eq!(home.address_of("chiyo"), Some("poll"), "the route ends at a poll node");
    assert_eq!(home.gates()[AWAY], "sakaki");
    assert_eq!(away.gates()[HOME], "sakaki", "the gate is answered both ways");
    assert_eq!(
        home.key_of("sakaki"),
        away.key_of("sakaki"),
        "one node sits in both meshes with ONE identity key"
    );
    assert_eq!(home.name_of_key(home.key_of("sakaki").unwrap()), Some("sakaki"));
}

#[test]
fn a_one_sided_gate_charter_is_refused_at_load() {
    let _lock = env_lock();
    let (_env, fx) = fixture("one-sided");
    // A box that holds both meshes takes the good charters, then the broken
    // `away` one on top of them.
    fx.enter("sakaki");
    fx.take_broken_away("sakaki", ONE_SIDED_GATE);

    let set = routing::declarations();
    let home = set.iter().find(|l| l.mesh == HOME).expect("home is in the set");
    let refusal = home.declaration.as_ref().err().expect("home's gate is un-answered");
    assert_eq!(refusal.reason, routing::ONE_SIDED_GATE, "{refusal}");
    assert!(refusal.detail.contains(AWAY) && refusal.detail.contains("sakaki"), "{refusal}");
    let away = set.iter().find(|l| l.mesh == AWAY).expect("away is in the set");
    assert!(away.declaration.is_ok(), "the mesh that declared no gate still loads: {away:?}");
}

#[test]
fn one_node_name_with_two_keys_across_the_two_meshes_is_refused_at_load() {
    let _lock = env_lock();
    let (_env, fx) = fixture("two-keys");
    fx.enter("sakaki");
    fx.take_broken_away("sakaki", TWO_KEYS);

    let set = routing::declarations();
    let refusal = set
        .iter()
        .filter_map(|l| l.declaration.as_ref().err())
        .find(|r| r.reason == routing::KEY_DIVERGENCE)
        .unwrap_or_else(|| panic!("one node with two keys is refused: {set:?}"));
    assert!(refusal.detail.contains("sakaki"), "{refusal}");
}

/// The edges the fixture declares are the edges the route takes: the letters
/// go osaka → sakaki → chiyo, with the last leg held at the relay, and the
/// gate is the only hop that rewrites a letter's mesh.
#[test]
fn the_fixture_edges_route_through_the_declaration_seam() {
    let _lock = env_lock();
    let (_env, fx) = fixture("route");

    let hop = |from: &str, to: &str, mesh: &str| {
        fx.enter(from);
        routing::Letter { from, to, mesh }.route(&routing::declarations())
    };

    // A plain member cannot dial the `poll` node: the letter goes to the relay.
    let first = hop("osaka", "chiyo", HOME);
    let hop_to_relay = first.outcome.as_ref().unwrap_or_else(|e| panic!("{e}\n{}", first.trail.join("\n")));
    assert_eq!(hop_to_relay.next, "sakaki", "{}", first.trail.join("\n"));
    assert!(!hop_to_relay.held);
    assert_eq!(hop_to_relay.mesh, HOME);

    // At the relay the same letter is held for `chiyo`'s own ask.
    let second = hop("sakaki", "chiyo", HOME);
    let held = second.outcome.as_ref().unwrap_or_else(|e| panic!("{e}\n{}", second.trail.join("\n")));
    assert_eq!(held.next, "chiyo", "{}", second.trail.join("\n"));
    assert!(held.held, "a `poll` destination is held at its relay: {}", second.trail.join("\n"));

    // Across the meshes only the gate crosses, and it rewrites the mesh it
    // carries: `evo` is a node of `away`, not of `home`.
    let bridged = hop("sakaki", "evo", HOME);
    let at_evo = bridged.outcome.as_ref().unwrap_or_else(|e| panic!("{e}\n{}", bridged.trail.join("\n")));
    assert_eq!(at_evo.next, "evo", "{}", bridged.trail.join("\n"));
    assert_eq!(at_evo.mesh, AWAY, "{}", bridged.trail.join("\n"));
}
