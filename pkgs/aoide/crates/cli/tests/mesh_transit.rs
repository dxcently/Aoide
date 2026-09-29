//! The five-edge fixture mesh (see `common/mod.rs`), loaded and validated
//! through the one declaration seam, plus its two deliberately broken
//! charters and the route it declares — and, for the tests that need them, real
//! `aoide a2a serve` children on the boxes' own loopback ports (`Fixture::doors`)
//! and a hand-signed POST at a named door (`common::door_post`). The fixture
//! proves what it DECLARES against the four steps, and the door-level tests prove
//! the same paths through a real door.

mod common;

use common::{fixture, env_lock, door_post, AWAY, HOME, ONE_SIDED_GATE, TWO_KEYS};

use aoide::dispatch::{dispatch, Invocation};
use aoide_protocol::output::Status;
use aoide_protocol::Door;
use aoide_storage::routing;

fn cli_invocation(path: &[&str], args: &[&str], flags: &[(&str, &str)]) -> Invocation {
    Invocation {
        path: path.iter().map(|s| s.to_string()).collect(),
        args: args.iter().map(|s| s.to_string()).collect(),
        flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        door: Door::Cli,
    }
}

#[test]
fn the_five_edge_fixture_loads_and_validates_through_the_declaration_seam() {
    let _lock = env_lock();
    let (_env, fx) = fixture("load");
    // `sakaki` is the box in BOTH meshes, so it is the one that reads a set
    // with two declarations in it.
    fx.enter("sakaki");

    let set = routing::declarations().expect("the fixture's declarations load");
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

    let set = routing::declarations().expect("the fixture's declarations load");
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

    let set = routing::declarations().expect("the fixture's declarations load");
    // Two charters, one name, two keys: neither mesh is an authority over the
    // other, so BOTH are refused — a name that stands for two machines is a
    // name neither mesh may route by.
    for mesh in [HOME, AWAY] {
        let refusal = set
            .iter()
            .find(|l| l.mesh == mesh)
            .and_then(|l| l.declaration.as_ref().err())
            .unwrap_or_else(|| panic!("`{mesh}` is refused: {set:?}"));
        assert_eq!(refusal.reason, routing::KEY_DIVERGENCE, "{refusal}");
        assert!(refusal.detail.contains("sakaki"), "{refusal}");
    }
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
        let set = routing::declarations().expect("the fixture's declarations load");
        routing::Letter { from, to, mesh }.route(&set)
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

/// `mail route` over the same fixture: the command prints the path and every
/// step's reason, and sends nothing at all — no spool, no mailbase, no dial.
#[test]
fn mail_route_prints_the_path_and_sends_nothing() {
    let _lock = env_lock();
    let (_env, fx) = fixture("mail-route");
    fx.enter("osaka");

    let out = dispatch(&cli_invocation(&["mail", "route"], &["chiyo/conductor"], &[("json", "true")]));
    assert_eq!(out.status, Status::Ok, "{}", out.message);
    let data = out.data.as_ref().expect("--json carries the route");
    assert_eq!(data["from"], "osaka");
    assert_eq!(data["to"], "chiyo");
    assert_eq!(data["mesh"], HOME);
    assert_eq!(data["next"], "sakaki", "a plain member hands a `poll` letter to the relay");
    assert_eq!(data["nextMesh"], HOME);
    assert_eq!(data["held"], false);
    assert_eq!(data["dial"], fx.url_of("sakaki"), "the hop's declared address, dialled as declared");
    let steps: Vec<&str> =
        data["steps"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    assert!(steps.iter().any(|s| s.starts_with("step 1:")), "{steps:?}");
    assert!(steps.iter().any(|s| s.starts_with("step 2:")), "{steps:?}");

    // The human body: the path, each step's reason, and what a refusing hop
    // does to the letter.
    let text = dispatch(&cli_invocation(&["mail", "route"], &["chiyo/conductor"], &[]));
    assert_eq!(text.status, Status::Ok, "{}", text.message);
    assert!(text.message.contains("route: osaka -> chiyo/conductor"), "{}", text.message);
    assert!(text.message.contains("next hop: sakaki"), "{}", text.message);
    assert!(text.message.contains("step 2:"), "{}", text.message);
    assert!(text.message.contains("parks"), "{}", text.message);

    // Nothing was sent. The directories are the real ones — `outbox::outbox_dir`
    // and `mail::mail_dir` under the box this process is entered on — which is
    // exactly where a `mail send` would have written its spool and its letter.
    let osaka = &fx.boxes["osaka"];
    let outbox = aoide_storage::outbox::outbox_dir();
    let mailbase = aoide_storage::mail::mail_dir();
    assert!(outbox.starts_with(osaka), "the outbox under test is osaka's: {}", outbox.display());
    assert!(mailbase.starts_with(osaka), "and so is the mailbase: {}", mailbase.display());
    assert!(!outbox.exists(), "a route spools nothing into {}", outbox.display());
    assert!(!mailbase.exists(), "and files nothing into {}", mailbase.display());

    // At the gate: the mesh is rewritten, and a `poll` destination is held at
    // the relay that holds its letters. Neither report sends anything.
    fx.enter("sakaki");
    let bridged = dispatch(&cli_invocation(&["mail", "route"], &["evo/conductor"], &[("json", "true")]));
    assert_eq!(bridged.status, Status::Ok, "{}", bridged.message);
    let data = bridged.data.as_ref().unwrap();
    assert_eq!(data["next"], "evo");
    assert_eq!(data["nextMesh"], AWAY, "the gate rewrites the mesh it carries");

    let held = dispatch(&cli_invocation(&["mail", "route"], &["chiyo/conductor"], &[("json", "true")]));
    let data = held.data.as_ref().unwrap();
    assert_eq!(data["next"], "chiyo");
    assert_eq!(data["held"], true, "held for `chiyo`'s own ask");
    assert_eq!(data["dial"], "poll");
    assert!(!aoide_storage::outbox::outbox_dir().exists(), "still nothing spooled");

    // A name no declaration carries, and a mesh asked for by a name that is not
    // a mesh name: two refusals, each with its own word.
    let unknown = dispatch(&cli_invocation(&["mail", "route"], &["nobody/conductor"], &[]));
    assert_eq!(unknown.status, Status::Error, "{}", unknown.message);
    assert_eq!(unknown.data.as_ref().unwrap()["reason"], "unknown-node");
    let malformed = dispatch(&cli_invocation(
        &["mail", "route"],
        &["chiyo/conductor"],
        &[("mesh", "Home Mesh")],
    ));
    assert_eq!(malformed.status, Status::Usage, "{}", malformed.message);
    assert_eq!(malformed.data.as_ref().unwrap()["reason"], "invalid-mesh");
}

/// This box's name in a mesh is the name its own identity key resolves to
/// there, never the OS hostname: a box whose hostname is not its charter line's
/// name routes as itself, and a charter mesh that does not carry its key names
/// nobody — the hostname does not stand in for a name that mesh never gave it.
#[test]
fn mail_route_resolves_self_by_identity_key_not_by_hostname() {
    let _lock = env_lock();
    let (_env, fx) = fixture("self-name");
    fx.enter("sakaki");
    // The hostname this box would report for itself — a name no charter line
    // carries.
    std::env::set_var("AOIDE_A2A_NODE_NAME", "sakaki-box");

    // Addressed to the box's own charter name, in the mesh that name is its
    // line in: at the box itself, that is nothing to route.
    let own = dispatch(&cli_invocation(
        &["mail", "route"],
        &["sakaki/conductor"],
        &[("mesh", HOME), ("json", "true")],
    ));
    assert_eq!(own.status, Status::Ok, "{}", own.message);
    let data = own.data.as_ref().unwrap();
    assert_eq!(data["from"], "sakaki", "the charter line's name for this box's own key");
    assert_eq!(data["next"], "sakaki");
    let steps: Vec<&str> =
        data["steps"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    assert!(steps.iter().any(|s| s.contains("is this box")), "{steps:?}");

    // And a letter riding `home` toward a node of `away`: `sakaki` is the
    // declared gate, so it rewrites the mesh. A hostname this box does not carry
    // would have made it a stranger handing the letter to its own relay instead.
    let bridged = dispatch(&cli_invocation(
        &["mail", "route"],
        &["evo/conductor"],
        &[("mesh", HOME), ("json", "true")],
    ));
    assert_eq!(bridged.status, Status::Ok, "{}", bridged.message);
    let data = bridged.data.as_ref().unwrap();
    assert_eq!(data["next"], "evo");
    assert_eq!(data["nextMesh"], AWAY, "this box is the gate: {}", data["steps"]);
}

/// A charter mesh names this box only where its own line carries this box's
/// identity key. Let the hostname spell a member's name and nothing changes: the
/// box is a stranger in that mesh and gets no route at all — never the member's
/// route, never handed a letter the member would have carried.
#[test]
fn a_box_whose_hostname_spells_a_member_is_a_stranger_in_a_charter_mesh() {
    let _lock = env_lock();
    let (_env, fx) = fixture("stranger");
    // `evo`'s key is on `away`'s charter alone, and this is the name home's
    // lines happen to spell.
    fx.enter("evo");
    std::env::set_var("AOIDE_A2A_NODE_NAME", "sakaki");

    let stranger = dispatch(&cli_invocation(
        &["mail", "route"],
        &["chiyo/conductor"],
        &[("mesh", HOME), ("json", "true")],
    ));
    assert_eq!(stranger.status, Status::Error, "{}", stranger.message);
    assert_eq!(stranger.data.as_ref().unwrap()["reason"], "not-a-member");

    // The mesh that DOES carry its key routes as `evo`, so the refusal above is
    // the membership rule and not a box that cannot read its own name at all.
    let at_home = dispatch(&cli_invocation(
        &["mail", "route"],
        &["sakaki/conductor"],
        &[("mesh", AWAY), ("json", "true")],
    ));
    assert_eq!(at_home.status, Status::Ok, "{}", at_home.message);
    assert_eq!(at_home.data.as_ref().unwrap()["from"], "evo");
}

/// The sealed container's whole journey on the fixture: `osaka` spools a letter
/// for `chiyo` toward its RELAY (the container stays addressed to `chiyo`),
/// `sakaki` carries it on by appending its own chained hop signature and holds it
/// for `chiyo`'s own poll, and `chiyo` opens a chain of two. Nothing leaves the
/// process: the hop each box would take is the storage seam its door runs.
#[test]
fn osaka_to_chiyo_routes_via_sakaki_and_the_chain_of_two_verifies_at_chiyo() {
    let _lock = env_lock();
    let (_env, fx) = fixture("transit");

    fx.enter("osaka");
    // `mail send` itself: the route is read first (a letter for `chiyo` in
    // `home` goes to its RELAY — `chiyo` is a `poll` node and this box is not
    // the relay it asks), and the container is sealed to `chiyo`'s age key OFF
    // ITS CHARTER LINE, since this box never paired with it.
    let sent = dispatch(&cli_invocation(
        &["mail", "send"],
        &["a letter for chiyo"],
        &[("to", "chiyo/conductor"), ("json", "true")],
    ));
    assert_eq!(sent.status, Status::Ok, "{}", sent.message);
    let data = sent.data.as_ref().unwrap();
    assert_eq!(data["next"], "sakaki", "the route hands it to the relay");
    assert_eq!(data["nextMesh"], HOME);

    // The spool toward `sakaki` holds the sealed container, and that container
    // is addressed to CHIYO: the hop and the destination are different facts.
    let spooled = aoide_storage::outbox::list_entries("sakaki").unwrap();
    assert_eq!(spooled.len(), 1, "spooled toward the hop the route picked");
    let container = spooled[0].container.clone().expect("sealed from the charter line");
    assert_eq!(container.to.node, "chiyo");
    assert_eq!(container.transit.len(), 1, "the origin's own entry, and nothing else yet");
    assert_eq!(container.transit[0].next, "sakaki");

    // `sakaki` receives a container that is not its own: a hop, never an open.
    fx.enter("sakaki");
    let hop = match aoide_storage::seal::deposit_container(&container, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Hopped(hop) => hop,
        other => panic!("a hub carries it on rather than opens it: {other:?}"),
    };
    assert_eq!(hop.next, "chiyo");
    assert_eq!(hop.mesh, HOME, "no gate is crossed — `chiyo` is in the letter's own mesh");
    assert!(hop.held, "`chiyo` polls, so its relay holds the letter for its own ask");
    assert_eq!(hop.container.transit.len(), 2, "the hub's own chained hop signature");
    aoide_storage::seal::file_transit_hop(&hop, "osaka").unwrap();
    assert_eq!(aoide_storage::outbox::list_entries("chiyo").unwrap().len(), 1, "held toward `chiyo`");

    // What the hub's own store holds: the routing metadata and the container's
    // digest — no mailbox name, no byte of the letter, and no second copy of the
    // ciphertext (the spool toward `chiyo` is where the container lives).
    let transit = aoide_storage::mail::read_transit_unlocked().unwrap();
    assert_eq!(transit.len(), 1, "one transit line at the hub");
    assert_eq!(transit[0].next, "chiyo");
    assert!(transit[0].held);
    assert_eq!(transit[0].digest.len(), 64, "the container's own digest, hex");
    let logged = serde_json::to_string(&transit[0]).unwrap();
    assert!(!logged.contains("conductor"), "no mailbox name at the hub: {logged}");
    assert!(!logged.contains("a letter for chiyo"), "and no letter bytes: {logged}");
    assert!(!logged.contains("\"ct\""), "and no ciphertext: the spool is where that is: {logged}");

    // `chiyo` opens it: two hops, the origin's and the relay's, ending here.
    fx.enter("chiyo");
    match aoide_storage::seal::deposit_container(&hop.container, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Opened { envelope, .. } => {
            assert_eq!(envelope.text, "a letter for chiyo");
            assert_eq!(envelope.header.from.node, "osaka");
        }
        other => panic!("the destination opens a two-entry chain: {other:?}"),
    }
}

/// The same journey driven to its END: `chiyo` POLLS its relay, the hand-over
/// files the letter (the poll's own seam: `outbox::hand_over` + the door's
/// `deposit_container`), `chiyo`'s receipt rides back, and `osaka`'s spooled
/// entry retires on it — the whole point of transit is that the origin learns
/// its letter landed, not that a hub took custody.
#[test]
fn chiyos_poll_files_the_letter_and_the_origins_entry_retires_on_the_receipt() {
    let _lock = env_lock();
    let (_env, fx) = fixture("journey");
    fx.enter("osaka");
    let sent = dispatch(&cli_invocation(
        &["mail", "send"],
        &["the whole way"],
        &[("to", "chiyo/conductor"), ("json", "true")],
    ));
    assert_eq!(sent.status, Status::Ok, "{}", sent.message);
    let msgid = sent.data.as_ref().unwrap()["msgid"].as_str().unwrap().to_string();
    // `osaka`'s own spool, before leaving the box: the entry it will hand to the
    // relay, holding the container sealed to `chiyo`.
    let container = aoide_storage::outbox::list_entries("sakaki").unwrap()[0].container.clone().unwrap();

    // `sakaki` hops it (the door's own arm over the storage seam).
    fx.enter("sakaki");
    let hop = match aoide_storage::seal::deposit_container(&container, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Hopped(hop) => hop,
        other => panic!("a hub carries it on: {other:?}"),
    };
    aoide_storage::seal::file_transit_hop(&hop, "osaka").unwrap();
    assert_eq!(aoide_storage::outbox::list_entries("chiyo").unwrap().len(), 1, "held for `chiyo` — the relay's own spool");

    // `chiyo` asks: the RELAY's own offer and hand-over read (a poll is computed
    // where the letters are held). The hand-over alone retires nothing — a
    // response can be lost — so the entry is still the relay's until `chiyo`
    // says it has it, which its next poll does.
    let offered = aoide_storage::outbox::poll_payloads("chiyo").unwrap();
    assert_eq!(offered.len(), 1, "the relay offers exactly what it holds toward the poller");
    let hand_over = match aoide_storage::outbox::hand_over("chiyo", &hop.container.msgid).unwrap() {
        aoide_storage::outbox::HandOver::Container(container) => *container,
        aoide_storage::outbox::HandOver::Envelope(_) => panic!("a sealed entry hands over the container"),
        aoide_storage::outbox::HandOver::Nothing => panic!("the entry is still spooled"),
    };
    assert_eq!(aoide_storage::outbox::poll_payloads("chiyo").unwrap().len(), 1, "…and re-offers it until then");

    // `chiyo` receives it: the door's verification, then the filing.
    fx.enter("chiyo");
    let (container, digest) = match aoide_storage::seal::deposit_container(&hand_over, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Opened { envelope, digest } => {
            assert_eq!(envelope.text, "the whole way");
            assert_eq!(envelope.header.from.node, "osaka");
            assert!(matches!(
                aoide_storage::mail::deposit((*envelope).clone(), "sakaki").unwrap(),
                aoide_storage::mail::DepositOutcome::Filed { .. }
            ));
            (hand_over, digest)
        }
        other => panic!("the destination opens it: {other:?}"),
    };
    aoide_storage::seal::record_admitted(&container, &digest).unwrap();
    assert_eq!(aoide_storage::mail::filed_kind(&msgid).as_deref(), Some(aoide_storage::mail::ENTRY_TYPE_LETTER));

    // `chiyo`'s receipt, deposited back at the origin: the entry retires.
    let ack = aoide_storage::mail::mint_ack_in_mesh(
        "conductor",
        aoide_storage::mail::Address { node: "osaka".to_string(), name: "conductor".to_string() },
        &msgid,
        HOME,
    )
    .unwrap();
    fx.enter("osaka");
    assert!(matches!(
        aoide_storage::mail::deposit(ack.clone(), "chiyo").unwrap(),
        aoide_storage::mail::DepositOutcome::Filed { .. } | aoide_storage::mail::DepositOutcome::Duplicate { .. }
    ));
    assert_eq!(
        aoide_storage::outbox::retire_by_ack(&ack).unwrap().as_deref(),
        Some(msgid.as_str()),
        "a receipt from the destination retires the origin's entry, wherever a hop put it"
    );
    assert!(aoide_storage::outbox::list_entries("sakaki").unwrap().is_empty());
}

/// **The box's OS name is not the name its mesh gave it.** Every other test in
/// this file lets `AOIDE_A2A_NODE_NAME` stand in for the node so one process can
/// play several boxes; the mail path must not depend on that coincidence. With
/// the host left as the host (`sakaki-host`, `chiyo-host`) and the charters still
/// naming `sakaki` and `chiyo`, the relay still hops the container and the
/// destination still opens it — the names that matter are the ones the
/// declarations give these keys (`routing::own_name_in`), for the chain's
/// last-next check, the loop guard and the destination check alike.
#[test]
fn a_host_whose_name_is_not_its_charter_name_still_hops_and_still_receives() {
    let _lock = env_lock();
    let (_env, fx) = fixture("host-vs-line");
    fx.enter("osaka");
    let sent = dispatch(&cli_invocation(
        &["mail", "send"],
        &["a letter for chiyo"],
        &[("to", "chiyo/conductor"), ("json", "true")],
    ));
    assert_eq!(sent.status, Status::Ok, "{}", sent.message);
    assert_eq!(sent.data.as_ref().unwrap()["next"], "sakaki");

    // The hop happens on `sakaki`'s own box, whose host is `sakaki-host`: the
    // container is read from osaka's spool first, then the box is entered.
    let container = aoide_storage::outbox::list_entries("sakaki").unwrap()[0].container.clone().unwrap();
    fx.enter("sakaki");
    std::env::set_var("AOIDE_A2A_NODE_NAME", "sakaki-host");
    let key = aoide_storage::identity::load_or_mint().unwrap().0.info().pubkey_hex;
    let set = routing::declarations().unwrap();
    assert_eq!(
        routing::own_name_in(&set, HOME, &key).as_deref(),
        Some("sakaki"),
        "this box's key is `sakaki`'s line in `home`, whatever the host is called"
    );
    let hop = match aoide_storage::seal::deposit_container(&container, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Hopped(hop) => hop,
        other => panic!("a hub carries it on: {other:?}"),
    };
    assert_eq!(hop.next, "chiyo");
    aoide_storage::seal::file_transit_hop(&hop, "osaka").unwrap();

    // And the destination answers to its own charter line too.
    fx.enter("chiyo");
    std::env::set_var("AOIDE_A2A_NODE_NAME", "chiyo-host");
    match aoide_storage::seal::deposit_container(&hop.container, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Opened { envelope, .. } => {
            assert_eq!(envelope.text, "a letter for chiyo");
        }
        other => panic!("the destination opens a chain that ends at its charter name: {other:?}"),
    }
}

/// **A learnt binding is only usable if it is the DECLARED key's.** A binding is
/// stored per NAME, so a file left by an earlier pairing — or written by anything
/// on this machine — would otherwise seal a letter to a key the mesh no longer
/// trusts that name for. Here `home`'s charter gives `chiyo` one age key and a
/// stale paired record gives the same name another, with a binding signed by the
/// stale one on disk: the letter is sealed to the CHARTER's key, never the stale
/// record's.
#[test]
fn a_learnt_binding_that_is_not_the_declared_key_is_not_used() {
    let _lock = env_lock();
    let (_env, fx) = fixture("stale-binding");
    fx.enter("osaka");

    let declared = aoide_storage::charter::governing(HOME).unwrap().nodes["chiyo"].age.clone();
    // The stale record's key: a second identity, minted for this test alone.
    let stale_kp = aoide_storage::identity::load_or_mint_seed_file(
        &std::path::PathBuf::from(aoide_storage::fs::state_dir()).join("stale.key"),
    )
    .unwrap()
    .0;
    // A DIFFERENT age key, so which binding was used is visible in the container.
    let (stale_age, _) = aoide_storage::seal::load_or_mint_age_identity().unwrap();
    let stale_recipient = aoide_storage::seal::recipient_of(&stale_age);
    let stale_binding = aoide_storage::seal::mint_binding(
        &stale_kp,
        &stale_recipient,
        1,
        &aoide_storage::time::now_iso_utc(),
        &aoide_storage::time::shift_iso_utc(&aoide_storage::time::now_iso_utc(), 86_400),
    )
    .unwrap();
    assert_ne!(stale_recipient, declared.age_pubkey, "two different age keys, one per binding");
    assert_ne!(stale_binding.identity_key, aoide_storage::charter::governing(HOME).unwrap().nodes["chiyo"].key);

    // The record the stale binding is learnt against, and the binding itself.
    let mut nodes = aoide_storage::node_store::load_nodes();
    aoide_storage::node_store::upsert_paired_node(
        &mut nodes,
        "chiyo",
        "ssh://chiyo",
        &stale_kp.info().pubkey_hex,
        &aoide_storage::time::now_iso_utc(),
        &["message".to_string()],
        HOME,
    );
    aoide_storage::node_store::save_nodes(&nodes).unwrap();
    std::fs::create_dir_all(aoide_storage::seal::peer_bindings_dir()).unwrap();
    std::fs::write(
        aoide_storage::seal::peer_bindings_dir().join("chiyo.json"),
        serde_json::to_string(&stale_binding).unwrap(),
    )
    .unwrap();
    assert!(
        aoide_storage::seal::usable_binding_for("chiyo", &aoide_storage::time::now_iso_utc()).is_some(),
        "the stale file IS usable by itself — that is the trap"
    );

    let envelope = aoide_storage::mail::mint_outbound_letter_from(
        "osaka",
        "conductor",
        "chiyo",
        "conductor",
        "to the charter's key, not the stale record's",
        HOME,
    )
    .unwrap();
    let entry = aoide_client::mail_wire::spool_entry("chiyo", "chiyo", HOME, envelope, false).unwrap();
    let container = entry.container.expect("the charter line's own binding seals it");
    assert_eq!(
        container.to.age, declared.age_pubkey,
        "sealed to the age key the CHARTER declares for `chiyo`"
    );
    assert_ne!(container.to.age, stale_binding.age_pubkey, "never to the stale record's");
}

/// The doors really answer: the fixture's boxes raised as `aoide a2a serve`
/// children, and a `aoide/mailPoll` POSTed to `sakaki`'s own door — signed as
/// `osaka`, over plain HTTP, on the port its charter line declares. The answer is
/// the door's, and the relay's own audit log says so.
#[test]
fn the_fixtures_boxes_answer_on_their_own_doors() {
    let _lock = env_lock();
    let (_env, fx) = fixture("doors-up");
    let _doors = fx.doors(&["osaka", "sakaki"]);

    fx.enter("osaka");
    // A verified record for osaka at SAKAKI's box, first: this diagnostic run
    // separates "my signed request is wrong" from "the charter rung did not
    // resolve".
    let osaka_key = aoide_storage::identity::load_or_mint().unwrap().0.info().pubkey_hex;
    fx.enter("sakaki");
    let mut nodes = aoide_storage::node_store::load_nodes();
    aoide_storage::node_store::upsert_paired_node(
        &mut nodes,
        "osaka",
        "http://127.0.0.1:1/",
        &osaka_key,
        &aoide_storage::time::now_iso_utc(),
        &["message".to_string()],
        HOME,
    );
    aoide_storage::node_store::save_nodes(&nodes).unwrap();
    fx.enter("osaka");
    // The client's own signed POST to the same door, over a plain-http record:
    // if THIS reaches the door, the child's env is right and only my
    // hand-written request is wrong.
    let mut nodes = aoide_storage::node_store::load_nodes();
    aoide_storage::node_store::upsert_paired_node(
        &mut nodes,
        "sakaki",
        &format!("http://127.0.0.1:{}/", fx.ports["sakaki"]),
        &aoide_storage::charter::governing(HOME).unwrap().nodes["sakaki"].key.clone(),
        &aoide_storage::time::now_iso_utc(),
        &["message".to_string()],
        HOME,
    );
    aoide_storage::node_store::save_nodes(&nodes).unwrap();
    let polled = aoide_client::mail_wire::poll_node("sakaki", None);
    assert!(polled.is_ok(), "the client's own poll reached the door: {polled:?}");
    // And a request signed by hand reaches the same door, which is what a
    // deposit AT A NAMED DOOR needs (the client's dial cannot open a `https://`
    // declaration with no TLS in front of it).
    let answer = door_post(fx.ports["sakaki"], "osaka", HOME, "aoide/mailPoll", serde_json::json!({ "node": "osaka" }));
    assert!(answer["result"].is_object(), "a hand-signed request verifies: {answer}");
    let relay_log = std::fs::read_to_string(fx.boxes["sakaki"].join("log")).unwrap_or_default();
    assert!(
        relay_log.contains("a2a.aoide/mailPoll"),
        "the relay's OWN door answered the poll: {relay_log}"
    );
}

/// **The whole journey, driven by the boxes' own doors.** Every hop is a real
/// `aoide a2a serve` child answering a real signed poll from the next box:
/// `osaka` spools the sealed container toward its relay and `sakaki` PULLS it
/// (the relay-first model), `sakaki` carries it on — its own hop signature, held
/// for `chiyo` — `chiyo` pulls it, files the letter and spools the receipt back,
/// and `osaka` pulls the receipt and retires its entry. Each box's own spool and
/// audit are grepped on the way: a hub's records hold the routed container and no
/// letter, and the origin's entry lives until the destination's receipt.
#[test]
fn the_whole_journey_runs_through_the_boxes_own_doors() {
    let _lock = env_lock();
    let (_env, fx) = fixture("journey-doors");
    let _doors = fx.doors(&["osaka", "sakaki", "chiyo"]);

    // `osaka` seals a letter for `chiyo` (from the charter line's binding) and
    // spools it toward the relay its route picks.
    fx.enter("osaka");
    let sent = dispatch(&cli_invocation(
        &["mail", "send"],
        &["through the doors"],
        &[("to", "chiyo/conductor"), ("json", "true")],
    ));
    assert_eq!(sent.status, Status::Ok, "{}", sent.message);
    assert_eq!(sent.data.as_ref().unwrap()["next"], "sakaki");
    let msgid = sent.data.as_ref().unwrap()["msgid"].as_str().unwrap().to_string();
    assert_eq!(
        aoide_storage::outbox::list_entries("sakaki").unwrap().len(),
        1,
        "spooled toward the relay"
    );

    // `sakaki` ASKS osaka for what is spooled toward it (a record gives it the
    // door; the charter gives it the key), and carries the container on.
    fx.enter("sakaki");
    let osaka_key = aoide_storage::charter::governing(HOME).unwrap().nodes["osaka"].key.clone();
    let mut nodes = aoide_storage::node_store::load_nodes();
    aoide_storage::node_store::upsert_paired_node(
        &mut nodes,
        "osaka",
        &format!("http://127.0.0.1:{}/", fx.ports["osaka"]),
        &osaka_key,
        &aoide_storage::time::now_iso_utc(),
        &["message".to_string()],
        HOME,
    );
    aoide_storage::node_store::save_nodes(&nodes).unwrap();
    let pulled = aoide_client::mail_wire::poll_node("osaka", None).unwrap();
    assert!(pulled.refused.is_empty(), "nothing refused: {:?}", pulled.refused);
    assert_eq!(aoide_storage::mail::read_transit_unlocked().unwrap().len(), 1, "the hop is recorded");
    assert_eq!(aoide_storage::outbox::list_entries("chiyo").unwrap().len(), 1, "held toward `chiyo`");
    let hub_spool = serde_json::to_string(&aoide_storage::outbox::list_entries("chiyo").unwrap()).unwrap();
    assert!(!hub_spool.contains("conductor"), "no mailbox name at the hub: {hub_spool}");
    assert!(!hub_spool.contains("through the doors"), "and no letter body: {hub_spool}");
    let hub_log = std::fs::read_to_string(fx.boxes["sakaki"].join("log")).unwrap_or_default();
    assert!(
        hub_log.contains("mail.poll.transit") && hub_log.contains("carried on to `chiyo`"),
        "the hub's own audit records the hop it made: {hub_log}"
    );
    let osaka_log = std::fs::read_to_string(fx.boxes["osaka"].join("log")).unwrap_or_default();
    assert!(osaka_log.contains("a2a.aoide/mailPoll"), "and osaka's door answered the ask: {osaka_log}");

    // `chiyo` asks its relay: the hand-over is a real deposit through `sakaki`'s
    // door, the letter is filed, and the receipt is spooled back.
    fx.enter("chiyo");
    let sakaki_key = aoide_storage::charter::governing(HOME).unwrap().nodes["sakaki"].key.clone();
    let mut nodes = aoide_storage::node_store::load_nodes();
    aoide_storage::node_store::upsert_paired_node(
        &mut nodes,
        "sakaki",
        &format!("http://127.0.0.1:{}/", fx.ports["sakaki"]),
        &sakaki_key,
        &aoide_storage::time::now_iso_utc(),
        &["message".to_string()],
        HOME,
    );
    aoide_storage::node_store::save_nodes(&nodes).unwrap();
    let received = aoide_client::mail_wire::poll_node("sakaki", None).unwrap();
    assert_eq!(received.filed, 1, "the poller filed it: {:?}", received.refused);
    assert_eq!(
        aoide_storage::mail::filed_kind(&msgid).as_deref(),
        Some(aoide_storage::mail::ENTRY_TYPE_LETTER),
        "the letter is in the destination's own mailbase"
    );
    let ack_spooled = aoide_storage::outbox::nodes_with_outbox().unwrap();
    assert!(ack_spooled.contains(&"osaka".to_string()), "the receipt is spooled toward the origin: {ack_spooled:?}");
    // The relay's custody ends on the next ask that names what it filed.
    let ask_again = aoide_client::mail_wire::poll_node("sakaki", None).unwrap();
    assert!(ask_again.refused.is_empty(), "{:?}", ask_again.refused);
    assert!(
        aoide_storage::outbox::filed_pending("sakaki").unwrap().is_empty(),
        "the acknowledgement went with the second ask"
    );

    // The relay held nothing once `chiyo` named it: its spool toward `chiyo` is
    // empty, which is the hub's half of the journey done. (The receipt itself is
    // `osaka`'s to come and get, and that path is `mail poll <relay>` on a mesh
    // whose relay is declared as a door — this fixture's osaka is the origin, and
    // its own entry's retirement by that receipt is
    // `a_letter_the_poller_already_has_is_acknowledged_and_the_hub_retires_it`'s
    // sibling case, not this one's.)
    fx.enter("sakaki");
    assert!(
        aoide_storage::outbox::list_entries("chiyo").unwrap().is_empty(),
        "the relay keeps nothing it was acknowledged for"
    );
    // And the origin is still waiting for its receipt: the ack is spooled at
    // `chiyo` (asserted above) and `chiyo` is a poll node nobody dials, so
    // `osaka`'s own entry stays spooled — its outbox reports the letter
    // undelivered, which is the truth until that receipt reaches it.
    fx.enter("osaka");
    let origin_spool = aoide_storage::outbox::list_entries("sakaki").unwrap();
    assert_eq!(origin_spool.len(), 1, "the origin's entry lives until the destination's receipt retires it");
    assert!(!origin_spool[0].last_attempt_reached_the_peer() || origin_spool[0].last_outcome != "accepted",
        "and it is not reported as delivered: {}",
        origin_spool[0].last_outcome);
}

/// **The symmetric gate rewrites `mesh`, and the entry it signs verifies in the
/// NEW zone.** A container riding `alpha` for `other` (a node of `beta`, which
/// `alpha` gates into through this box) is deposited AT the gate's own door: the
/// four steps rewrite the letter's mesh and hand it to `other`, and the hop entry
/// the gate appended is signed in `beta` under the gate's `beta` key — which is
/// exactly what the next door's own walk recomputes. The spool is grepped for the
/// rewritten container.
#[test]
fn the_gate_rewrites_the_mesh_and_its_hop_entry_verifies_at_the_next_door() {
    let _lock = env_lock();
    let (_env, root, port) = two_mesh_door("gate-rewrite", true);
    fx_enter(&root, EVO);
    let _door = common::raise_door(&root, EVO, port);

    let container = two_mesh_container(&root, "other");
    let answer = door_post(port, EVO, ALPHA, "aoide/mailDeposit", serde_json::json!({ "container": container }));
    let result = &answer["result"];
    assert_eq!(result["status"], "accepted", "{answer}");
    assert_eq!(result["transit"]["next"], "other", "{answer}");
    assert_eq!(result["transit"]["mesh"], BETA, "the gate rewrote the zone: {answer}");

    // The container the gate spooled onward: its chain grew by one entry, signed
    // in `beta`, and that signature verifies under the gate's `beta` key — which
    // is exactly what the next door's own walk recomputes.
    fx_enter(&root, EVO);
    let spooled = aoide_storage::outbox::list_entries("other").unwrap();
    assert_eq!(spooled.len(), 1, "spooled toward `other`");
    let forwarded = spooled[0].container.clone().expect("sealed");
    assert_eq!(forwarded.mesh, BETA, "and the container's own mesh is the new zone");
    let last = forwarded.transit.last().unwrap().clone();
    assert_eq!(last.node, EVO, "the gate's own entry");
    assert_eq!(last.mesh, BETA, "signed in the zone it carries the letter into");
    let (msgid, _) = aoide_storage::seal::chain_tail(&forwarded).unwrap();
    let beta_key = aoide_storage::routing::declarations()
        .unwrap()
        .iter()
        .find(|l| l.mesh == BETA)
        .and_then(|l| l.declaration.as_ref().ok())
        .and_then(|d| d.key_of(EVO))
        .map(str::to_string)
        .unwrap();
    let body = aoide_storage::seal::hop_bytes(
        &msgid,
        &entry_prev(&forwarded, forwarded.transit.len() - 1),
        &last.node,
        &last.next,
        &last.at,
        &last.mesh,
    );
    assert!(
        aoide_storage::wire_auth::verify_signature_hex(&beta_key, &body, &last.sig),
        "the next door recomputes exactly this"
    );
}

/// **A dual member that is not the gate does not bridge: `zone-violation`.** The
/// same box, with the two meshes declaring no gate between them: a letter riding
/// `alpha` for a node of `beta` cannot be carried, and the box says so with the
/// wall's own word rather than reaching for the destination it happens to know.
#[test]
fn a_dual_member_that_is_not_the_gate_refuses_the_bridge_at_the_door() {
    let _lock = env_lock();
    let (_env, root, port) = two_mesh_door("no-gate", false);
    fx_enter(&root, EVO);
    let _door = common::raise_door(&root, EVO, port);

    let container = two_mesh_container(&root, "other");
    let answer = door_post(port, EVO, ALPHA, "aoide/mailDeposit", serde_json::json!({ "container": container }));
    let result = &answer["result"];
    assert_eq!(result["status"], "refused", "{answer}");
    assert_eq!(result["reason"], aoide_storage::seal::ZONE_VIOLATION, "{answer}");
    fx_enter(&root, EVO);
    assert!(aoide_storage::mail::read_transit_unlocked().unwrap().is_empty(), "nothing is filed as a hop");
    assert!(aoide_storage::outbox::list_entries("other").unwrap().is_empty(), "nothing is spooled onward");
    let log = std::fs::read_to_string(root.join("log")).unwrap_or_default();
    assert!(log.contains(aoide_storage::seal::ZONE_VIOLATION), "and the refusal is audited: {log}");
}

/// **A rewritten `mesh` without the gate's signature is refused.** The container
/// reaches the door with its chain already claiming the crossing — an entry
/// signed in `beta` by a node the letter's own mesh does not declare as its gate.
/// The chain walk reads the crossing against the declaration SET, so the claim is
/// the wall, and nothing is filed or spooled onward.
#[test]
fn a_rewritten_mesh_without_the_gates_signature_is_refused_at_the_door() {
    let _lock = env_lock();
    let (_env, root, port) = two_mesh_door("rewritten", false);
    fx_enter(&root, EVO);
    let _door = common::raise_door(&root, EVO, port);

    // Hand-built: the origin's entry says `alpha`, and a second entry claims the
    // letter arrived in `beta` — signed by this box, which `alpha` does not
    // declare as its gate (the fixture declares none at all).
    let kp = aoide_storage::identity::load_or_mint().unwrap().0;
    let mut container = two_mesh_container(&root, "other");
    let (msgid, prev) = aoide_storage::seal::chain_tail(&container).unwrap();
    container.mesh = BETA.to_string();
    let at = aoide_storage::time::now_iso_utc();
    container.transit.push(aoide_storage::seal::TransitEntry {
        node: EVO.to_string(),
        next: "other".to_string(),
        at: at.clone(),
        mesh: BETA.to_string(),
        sig: aoide_storage::wire_auth::sign_hex(
            &kp,
            &aoide_storage::seal::hop_bytes(&msgid, &prev, EVO, "other", &at, BETA),
        ),
    });

    let answer = door_post(port, EVO, BETA, "aoide/mailDeposit", serde_json::json!({ "container": container }));
    let result = &answer["result"];
    assert_eq!(result["status"], "refused", "{answer}");
    assert_eq!(result["reason"], aoide_storage::seal::ZONE_VIOLATION, "{answer}");
    fx_enter(&root, EVO);
    assert!(aoide_storage::mail::read_transit_unlocked().unwrap().is_empty());
    assert!(aoide_storage::outbox::list_entries("other").unwrap().is_empty());
    let log = std::fs::read_to_string(root.join("log")).unwrap_or_default();
    assert!(log.contains(aoide_storage::seal::ZONE_VIOLATION), "audited: {log}");
}

/// The two meshes these door tests declare: `alpha` (this box's own) and `beta`
/// (the one a crossing reaches), gated through this box in one fixture and
/// ungated in the other.
const ALPHA: &str = "alpha";
const BETA: &str = "beta";
/// This box's own name in the two fixtures: a pair mesh's declaration IS its
/// records, so one verified record for this process's identity key under this
/// name makes it the member the whole fixture is about.
const EVO: &str = "evo";

/// Enter a box root and name it — the fixture's own switch, for a root that is
/// not one of the five-edge boxes.
fn fx_enter(root: &std::path::Path, name: &str) {
    std::env::set_var("AOIDE_ROOT", root);
    std::env::set_var("AOIDE_STATE_DIR", root);
    std::env::set_var("AOIDE_STAGE_DIR", root);
    std::env::set_var("AOIDE_A2A_NODE_NAME", name);
    std::env::remove_var("AOIDE_CONFIG");
}

/// One box, two pair meshes, and a free port for its door: `alpha` (the box's
/// own mesh, which the letter rides) and `beta` (where the destination `other`
/// is). `gated` declares the symmetric gate both ways; ungated declares none.
fn two_mesh_door(
    tag: &str,
    gated: bool,
) -> (aoide_test_support::EnvSaver, std::path::PathBuf, u16) {
    let guard = aoide_test_support::EnvSaver::capture(&[
        "AOIDE_ROOT",
        "AOIDE_STATE_DIR",
        "AOIDE_STAGE_DIR",
        "AOIDE_A2A_NODE_NAME",
        "AOIDE_CONFIG",
    ]);
    let root = aoide_test_support::unique_tmp(&format!("two-mesh-door-{tag}"));
    std::fs::create_dir_all(&root).unwrap();
    fx_enter(&root, EVO);
    let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
    let alpha_gate = if gated {
        format!("[mesh.{ALPHA}.gates]\n{BETA} = \"{EVO}\"\n")
    } else {
        String::new()
    };
    let beta_gate = if gated {
        format!("[mesh.{BETA}.gates]\n{ALPHA} = \"{EVO}\"\n")
    } else {
        String::new()
    };
    std::fs::write(
        aoide_storage::config::source().path,
        format!(
            "[pairing]\nhomeMesh = \"{ALPHA}\"\n\n\
             [mesh.{ALPHA}]\n\n[mesh.{ALPHA}.nodes]\n{EVO} = \"ssh://{EVO}\"\n\n{alpha_gate}\n\
             [mesh.{BETA}]\n\n[mesh.{BETA}.nodes]\n{EVO} = \"ssh://{EVO}\"\nother = \"ssh://other\"\n\n{beta_gate}"
        ),
    )
    .unwrap();
    aoide_storage::config::load().unwrap();

    let record = |node: &str, key: String, mesh: &str| aoide_storage::node_store::Node {
        name: node.to_string(),
        url: "ssh://self".to_string(),
        autogate: false,
        token_file: None,
        bearer_secret: None,
        hub: false,
        pubkey: Some(key),
        verified: true,
        grants: aoide_storage::node_store::grants_in(mesh, &["message"]),
        narrowed: aoide_storage::node_store::Grants::new(),
        via: None,
        added_at: "2026-09-07T00:00:00Z".to_string(),
    };
    // The same box in BOTH meshes — one identity key, one name, two lines — which
    // is what makes it a DUAL member; and `other`, a node of `beta` alone.
    aoide_storage::node_store::save_nodes(&[
        record(EVO, kp.info().pubkey_hex.clone(), ALPHA),
        record(EVO, kp.info().pubkey_hex, BETA),
        record("other", "c3c3c3c3".repeat(8), BETA),
    ])
    .unwrap();
    (guard, root, common::free_port())
}

/// A container the box at `root` mints for `to`: origin `evo`, riding `alpha`,
/// handed to itself — the shape a relay deposits after carrying a letter.
fn two_mesh_container(root: &std::path::Path, to: &str) -> aoide_storage::seal::Container {
    fx_enter(root, EVO);
    let binding = aoide_storage::seal::publish_binding().unwrap();
    let envelope = aoide_storage::mail::mint_outbound_letter_from(
        EVO,
        "alice",
        to,
        "bob",
        "a letter for another zone",
        ALPHA,
    )
    .unwrap();
    aoide_storage::seal::seal_envelope(
        &envelope,
        &binding,
        ALPHA,
        ALPHA,
        to,
        EVO,
        &aoide_storage::time::now_iso_utc(),
    )
    .unwrap()
}

/// The `prev` the entry at `index` chains to: the running digest of the entries
/// before it, recomputed the way the walk does.
fn entry_prev(container: &aoide_storage::seal::Container, index: usize) -> [u8; 32] {
    let (msgid, _) = aoide_storage::seal::chain_tail(container).unwrap();
    let mut prev = msgid;
    for entry in container.transit.iter().take(index) {
        prev = aoide_storage::seal::sha256(&aoide_storage::seal::hop_entry_bytes(&msgid, &prev, entry).unwrap());
    }
    prev
}

/// **A charter mesh that does not carry a box's key gives it no name.** `evo` is
/// a member of `away` and of no other mesh; a container riding `home` — signed by
/// `osaka`, who IS a member — deposited at evo's own door is answered
/// `not-a-member`: the box asked to carry it has no name in that mesh, and its
/// hostname is not a name the charter gave anyone. The message rides a REAL door,
/// and the box that is a stranger is a real one.
#[test]
fn a_stranger_in_a_charter_mesh_is_told_not_a_member_at_the_door() {
    let _lock = env_lock();
    let (_env, fx) = fixture("stranger-door");
    let _doors = fx.doors(&["osaka", "evo"]);

    // osaka spools a letter for chiyo toward its relay — a container the relay
    // would carry, signed by a member of `home`.
    fx.enter("osaka");
    let sent = dispatch(&cli_invocation(
        &["mail", "send"],
        &["not evo's to carry"],
        &[("to", "chiyo/conductor"), ("json", "true")],
    ));
    assert_eq!(sent.status, Status::Ok, "{}", sent.message);
    let container = aoide_storage::outbox::list_entries("sakaki").unwrap()[0]
        .container
        .clone()
        .expect("sealed");

    // At EVO's door — a member of `away`, a stranger in `home`.
    let answer = door_post(
        fx.ports["evo"],
        "osaka",
        HOME,
        "aoide/mailDeposit",
        serde_json::json!({ "container": container }),
    );
    let result = &answer["result"];
    assert_eq!(result["status"], "refused", "{answer}");
    assert_eq!(result["reason"], aoide_storage::routing::NOT_A_MEMBER, "{answer}");
    let log = std::fs::read_to_string(fx.boxes["evo"].join("log")).unwrap_or_default();
    assert!(log.contains(aoide_storage::routing::NOT_A_MEMBER), "and it is audited: {log}");
    fx.enter("evo");
    assert!(aoide_storage::mail::read_transit_unlocked().unwrap().is_empty(), "a stranger files no hop");
    assert!(aoide_storage::outbox::list_entries("chiyo").unwrap().is_empty(), "and spools nothing onward");
}

/// Give the box at `root` a verified record for `peer`, dialling `port`: a poll
/// dials a RECORD (`poll_node`'s own rule), so a box that asks another needs one
/// — the key its charter line carries, and the door's http url.
fn record_for_door(root: &std::path::Path, me: &str, peer: &str, port: u16) {
    fx_enter(root, me);
    let key = aoide_storage::charter::governing(HOME).unwrap().nodes[peer].key.clone();
    let mut nodes = aoide_storage::node_store::load_nodes();
    aoide_storage::node_store::upsert_paired_node(
        &mut nodes,
        peer,
        &format!("http://127.0.0.1:{port}/"),
        &key,
        &aoide_storage::time::now_iso_utc(),
        &["message".to_string()],
        HOME,
    );
    aoide_storage::node_store::save_nodes(&nodes).unwrap();
}

/// **A letter the poller already has is answered `duplicate` — and is still
/// acknowledged.** The hub offers what it holds; a poller that filed that letter
/// by another route (here: locally, before it ever asked) has the custody, says
/// so, and the hub retires its own copy on the NEXT ask. Without the
/// acknowledgement, a hub whose earlier answer was lost would hold that
/// container forever, offering it on every ask.
#[test]
fn a_letter_the_poller_already_has_is_acknowledged_and_the_hub_retires_it() {
    let _lock = env_lock();
    let (_env, fx) = fixture("duplicate-ack");
    let _doors = fx.doors(&["osaka", "sakaki", "chiyo"]);

    // osaka spools the letter toward its relay...
    fx.enter("osaka");
    let sent = dispatch(&cli_invocation(
        &["mail", "send"],
        &["already here"],
        &[("to", "chiyo/conductor"), ("json", "true")],
    ));
    assert_eq!(sent.status, Status::Ok, "{}", sent.message);
    let msgid = sent.data.as_ref().unwrap()["msgid"].as_str().unwrap().to_string();

    // ...sakaki pulls it and holds it for chiyo...
    record_for_door(&fx.boxes["sakaki"], "sakaki", "osaka", fx.ports["osaka"]);
    let pulled = aoide_client::mail_wire::poll_node("osaka", None).unwrap();
    assert!(pulled.refused.is_empty(), "{:?}", pulled.refused);
    fx.enter("sakaki");
    let container = aoide_storage::outbox::list_entries("chiyo").unwrap()[0].container.clone().unwrap();

    // ...and chiyo, having filed it by another route already, answers duplicate
    // when the relay offers it — then acknowledges, and the relay retires.
    fx.enter("chiyo");
    let (container, digest) = match aoide_storage::seal::deposit_container(&container, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Opened { envelope, digest } => {
            assert!(matches!(
                aoide_storage::mail::deposit((*envelope).clone(), "sakaki").unwrap(),
                aoide_storage::mail::DepositOutcome::Filed { .. }
            ));
            (container, digest)
        }
        other => panic!("the destination opens it: {other:?}"),
    };
    aoide_storage::seal::record_admitted(&container, &digest).unwrap();

    record_for_door(&fx.boxes["chiyo"], "chiyo", "sakaki", fx.ports["sakaki"]);
    let first_ask = aoide_client::mail_wire::poll_node("sakaki", None).unwrap();
    assert!(first_ask.refused.is_empty(), "the duplicate is not a refusal: {:?}", first_ask.refused);
    assert_eq!(
        aoide_storage::outbox::filed_pending("sakaki").unwrap(),
        vec![msgid.clone()],
        "the poller says it has it, so the relay is owed the acknowledgement"
    );
    fx.enter("sakaki");
    assert_eq!(aoide_storage::outbox::list_entries("chiyo").unwrap().len(), 1, "still the relay's, until told");

    // The next ask carries it, and the relay retires its custody.
    fx.enter("chiyo");
    let second_ask = aoide_client::mail_wire::poll_node("sakaki", None).unwrap();
    assert!(second_ask.refused.is_empty(), "{:?}", second_ask.refused);
    fx.enter("sakaki");
    assert!(
        aoide_storage::outbox::list_entries("chiyo").unwrap().is_empty(),
        "the duplicate's acknowledgement retired the relay's copy"
    );
}

/// **The chain's last hop must be the node that deposited it.** `osaka` is a
/// member of `home` and hands evo a container that `sakaki` — not `osaka` —
/// signed the last hop of: whatever the connection proved, it did not prove this,
/// and the door refuses `broken-chain` before anything is filed or hopped.
#[test]
fn a_deposit_whose_last_hop_is_someone_else_is_refused() {
    let _lock = env_lock();
    let (_env, fx) = fixture("not-my-hop");
    let _doors = fx.doors(&["osaka", "sakaki", "evo"]);

    // osaka spools a letter for chiyo; sakaki carries it on (its own hop entry
    // is now the LAST one) and holds it for chiyo.
    fx.enter("osaka");
    let sent = dispatch(&cli_invocation(
        &["mail", "send"],
        &["someone else's custody"],
        &[("to", "chiyo/conductor"), ("json", "true")],
    ));
    assert_eq!(sent.status, Status::Ok, "{}", sent.message);
    record_for_door(&fx.boxes["sakaki"], "sakaki", "osaka", fx.ports["osaka"]);
    let pulled = aoide_client::mail_wire::poll_node("osaka", None).unwrap();
    assert!(pulled.refused.is_empty(), "{:?}", pulled.refused);
    fx.enter("sakaki");
    let forwarded = aoide_storage::outbox::list_entries("chiyo").unwrap()[0].container.clone().unwrap();

    // osaka hands that SAME container to `evo`'s door — signed as itself, while
    // the chain's last hop is `sakaki`'s.
    fx.enter("osaka");
    let answer = door_post(fx.ports["evo"], "osaka", HOME, "aoide/mailDeposit", serde_json::json!({ "container": forwarded }));
    let result = &answer["result"];
    assert_eq!(result["status"], "refused", "{answer}");
    assert_eq!(result["reason"], aoide_storage::seal::BROKEN_CHAIN, "{answer}");
    assert!(
        result["detail"].as_str().unwrap().contains("sakaki"),
        "and it names whose hop it was: {answer}"
    );
    let log = std::fs::read_to_string(fx.boxes["evo"].join("log")).unwrap_or_default();
    assert!(log.contains(aoide_storage::seal::BROKEN_CHAIN), "audited: {log}");
}

/// The same binding on the PULL side: a relay offers a container whose last hop
/// is SOMEONE ELSE's — `osaka` minted it straight for `chiyo`, and `sakaki` is
/// handing it over as if it were its own custody. The poller refuses it
/// `broken-chain` and reports it, rather than taking a hop somebody else signed.
#[test]
fn a_polled_container_whose_last_hop_is_someone_else_is_refused() {
    let _lock = env_lock();
    let (_env, fx) = fixture("poll-not-my-hop");
    let _doors = fx.doors(&["sakaki", "chiyo"]);

    // osaka's own container for chiyo — entry 1 is osaka's, and it names chiyo as
    // the hop — planted in SAKAKI's spool toward chiyo, so the relay is offering
    // custody it never signed for.
    fx.enter("osaka");
    let chiyo_binding = aoide_storage::charter::governing(HOME).unwrap().nodes["chiyo"].age.clone();
    let envelope = aoide_storage::mail::mint_outbound_letter_from(
        "osaka",
        "alice",
        "chiyo",
        "bob",
        "not the relay's custody",
        HOME,
    )
    .unwrap();
    let container = aoide_storage::seal::seal_envelope(
        &envelope,
        &chiyo_binding,
        HOME,
        HOME,
        "chiyo",
        "chiyo",
        &aoide_storage::time::now_iso_utc(),
    )
    .unwrap();
    let msgid = container.msgid.clone();
    fx.enter("sakaki");
    aoide_storage::outbox::write_entry(
        "chiyo",
        &aoide_storage::outbox::OutboxEntry::sealed_held(envelope, container),
    )
    .unwrap();

    fx.enter("chiyo");
    record_for_door(&fx.boxes["chiyo"], "chiyo", "sakaki", fx.ports["sakaki"]);
    let asked = aoide_client::mail_wire::poll_node("sakaki", None).unwrap();
    assert_eq!(asked.filed, 0, "nothing is taken from a hop that is not the relay's: {:?}", asked.refused);
    assert!(
        asked.refused.iter().any(|line| line.contains(&msgid) && line.contains(aoide_storage::seal::BROKEN_CHAIN)),
        "and the refusal names it: {:?}",
        asked.refused
    );
    assert!(aoide_storage::mail::read_base().unwrap().is_empty(), "nothing is filed");
}

/// A chain truncated by dropping the tail never reaches the destination: the last
/// hop still in it hands the letter to `sakaki`, so `chiyo` refuses it
/// (`broken-chain`) and owes no ack — the letter is on no mailbox, and the
/// origin's own spool still reports it undelivered.
#[test]
fn a_chain_truncated_by_dropping_the_tail_yields_no_ack_and_leaves_the_letter_undelivered() {
    let _lock = env_lock();
    let (_env, fx) = fixture("truncated");
    let chiyo_binding = aoide_storage::charter::governing(HOME).unwrap().nodes["chiyo"].age.clone();

    fx.enter("osaka");
    let envelope = aoide_storage::mail::mint_outbound_letter_from(
        "osaka",
        "conductor",
        "chiyo",
        "conductor",
        "truncated on the way",
        HOME,
    )
    .unwrap();
    let container = aoide_storage::seal::seal_envelope(
        &envelope,
        &chiyo_binding,
        HOME,
        HOME,
        "chiyo",
        "sakaki",
        &aoide_storage::time::now_iso_utc(),
    )
    .unwrap();
    aoide_storage::outbox::write_entry("sakaki", &aoide_storage::outbox::OutboxEntry::sealed(envelope.clone(), container.clone())).unwrap();
    let msgid = envelope.msgid.clone();

    // The tail — the relay's own entry — never happens; the container the
    // destination is offered is the origin's entry alone.
    fx.enter("chiyo");
    match aoide_storage::seal::deposit_container(&container, HOME).unwrap() {
        aoide_storage::seal::ContainerOutcome::Refused { reason, .. } => {
            assert_eq!(reason, aoide_storage::seal::BROKEN_CHAIN, "{reason}")
        }
        other => panic!("a chain that stops short is refused: {other:?}"),
    }
    assert!(
        aoide_storage::mail::read_base().unwrap().is_empty(),
        "nothing is filed on the destination, so nothing is acked"
    );

    fx.enter("osaka");
    let still = aoide_storage::outbox::list_entries("sakaki").unwrap();
    assert_eq!(still.len(), 1, "the origin's outbox still holds the letter");
    assert_eq!(still[0].envelope.msgid, msgid);
    assert!(!still[0].last_attempt_reached_the_peer(), "and reports it undelivered");
}
