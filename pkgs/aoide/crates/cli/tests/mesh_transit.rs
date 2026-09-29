//! The five-edge fixture mesh (see `common/mod.rs`), loaded and validated
//! through the one declaration seam, plus its two deliberately broken
//! charters and the route it declares. No child process and no network: this
//! is the fixture proving ITSELF — that what it declares is what the four
//! steps read.

mod common;

use common::{fixture, env_lock, AWAY, HOME, ONE_SIDED_GATE, TWO_KEYS};

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
    assert_eq!(data["dial"], "ssh://sakaki");
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

    // What the hub's own store holds: the sealed container and its routing, and
    // neither a mailbox name nor a byte of the letter.
    let transit = aoide_storage::mail::read_transit_unlocked().unwrap();
    assert_eq!(transit.len(), 1, "one transit line at the hub");
    assert_eq!(transit[0].next, "chiyo");
    assert!(transit[0].held);
    let logged = serde_json::to_string(&transit[0]).unwrap();
    assert!(!logged.contains("conductor"), "no mailbox name at the hub: {logged}");
    assert!(!logged.contains("a letter for chiyo"), "and no letter bytes: {logged}");

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
