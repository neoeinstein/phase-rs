//! Chaotic Transformation: each permanent exiled this way drives its own
//! controller's reveal-until (CR 608.2c + CR 608.2h + CR 109.5), through the
//! `FilteredTrackedSetSize` per-member `repeat_for` rebind.

use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::types::card_type::CoreType;
use engine::types::events::{GameEvent, PlayerActionKind};
use engine::types::identifiers::ObjectId;
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::zones::Zone;

/// Chaotic Transformation, verified against Scryfall/MTGJSON oracle text.
const ORACLE: &str = "Exile up to one target artifact, up to one target creature, up to one target enchantment, up to one target planeswalker, and/or up to one target land. For each permanent exiled this way, its controller reveals cards from the top of their library until they reveal a card that shares a card type with it, puts that card onto the battlefield, then shuffles.";

struct Board {
    runner: GameRunner,
    spell: ObjectId,
    /// P0's artifact on the battlefield.
    artifact: ObjectId,
    /// P1's creature on the battlefield.
    creature: ObjectId,
    /// P0's library, top first: land, creature decoy, land, artifact.
    p0_library: Vec<ObjectId>,
    /// P1's library, top first: land, artifact decoy, creature.
    p1_library: Vec<ObjectId>,
}

/// Seed `player`'s library, top first, with generic cards of the given types.
fn seed_library(
    scenario: &mut GameScenario,
    player: PlayerId,
    cards_top_first: &[(&str, CoreType)],
) -> Vec<(ObjectId, CoreType)> {
    // Each add lands on top, so add bottom-first and restore top-first order.
    let mut seeded: Vec<_> = cards_top_first
        .iter()
        .rev()
        .map(|&(name, core)| (scenario.add_card_to_library_top(player, name), core))
        .collect();
    seeded.reverse();
    seeded
}

fn board() -> Board {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let creature = scenario.add_creature(P1, "Exiled Bear", 2, 2).id();
    let artifact = scenario
        .add_artifact_from_oracle(P0, "Exiled Trinket", "")
        .id();
    let spell = scenario
        .add_spell_to_hand_from_oracle(P0, "Chaotic Transformation", false, ORACLE)
        .id();
    let p0 = seed_library(
        &mut scenario,
        P0,
        &[
            ("P0 Top Land", CoreType::Land),
            ("P0 Creature Decoy", CoreType::Creature),
            ("P0 Second Land", CoreType::Land),
            ("P0 Artifact Hit", CoreType::Artifact),
        ],
    );
    let p1 = seed_library(
        &mut scenario,
        P1,
        &[
            ("P1 Top Land", CoreType::Land),
            ("P1 Artifact Decoy", CoreType::Artifact),
            ("P1 Creature Hit", CoreType::Creature),
        ],
    );
    let mut runner = scenario.build();
    for (id, core) in p0.iter().chain(p1.iter()) {
        let obj = runner.state_mut().objects.get_mut(id).unwrap();
        obj.card_types.core_types = vec![*core];
        obj.base_card_types = obj.card_types.clone();
    }
    Board {
        runner,
        spell,
        artifact,
        creature,
        p0_library: p0.into_iter().map(|(id, _)| id).collect(),
        p1_library: p1.into_iter().map(|(id, _)| id).collect(),
    }
}

/// How many times `player`'s library was shuffled during the resolution.
fn shuffles_of(events: &[GameEvent], player: PlayerId) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                GameEvent::PlayerPerformedAction {
                    player_id,
                    action: PlayerActionKind::ShuffledLibrary,
                    ..
                } if *player_id == player
            )
        })
        .count()
}

/// CR 608.2c + CR 608.2h + CR 109.5: each exiled permanent drives its own
/// reveal. The artifact's controller (P0) reveals until an ARTIFACT and the
/// creature's controller (P1) until a CREATURE; a decoy of the other member's
/// type sits above each hit, so a reveal that bound every iteration to the last
/// slot would put the wrong card onto the battlefield.
#[test]
fn each_exiled_permanent_drives_its_own_controllers_reveal() {
    let mut b = board();
    let outcome = b
        .runner
        .cast(b.spell)
        .target_objects(&[b.artifact, b.creature])
        .resolve();

    outcome.assert_zone(&[b.artifact, b.creature], Zone::Exile);
    let p0_hit = b.p0_library[3];
    let p1_hit = b.p1_library[2];
    outcome.assert_zone(&[p0_hit, p1_hit], Zone::Battlefield);
    outcome.assert_controls(P0, p0_hit);
    outcome.assert_controls(P1, p1_hit);
    // CR 701.20a: the revealed non-hits return to their library (then shuffled).
    outcome.assert_zone(
        &[b.p0_library[0], b.p0_library[1], b.p0_library[2]],
        Zone::Library,
    );
    outcome.assert_zone(&[b.p1_library[0], b.p1_library[1]], Zone::Library);
    // CR 701.24a: "then shuffles" is part of each member's iteration, so each
    // controller shuffles their own library exactly once.
    assert_eq!(shuffles_of(outcome.events(), P0), 1, "P0 shuffles once");
    assert_eq!(shuffles_of(outcome.events(), P1), 1, "P1 shuffles once");
}

/// One filled slot is one iteration: only the exiled creature's controller
/// reveals; the other player's library is untouched.
#[test]
fn a_single_filled_slot_reveals_only_for_that_member() {
    let mut b = board();
    let outcome = b
        .runner
        .cast(b.spell)
        .target_objects(&[b.creature])
        .resolve();

    outcome.assert_zone(&[b.creature], Zone::Exile);
    outcome.assert_zone(&[b.p1_library[2]], Zone::Battlefield);
    outcome.assert_controls(P1, b.p1_library[2]);
    outcome.assert_zone(&b.p0_library, Zone::Library);
    outcome.assert_zone(&[b.artifact], Zone::Battlefield);
    assert_eq!(shuffles_of(outcome.events(), P1), 1, "P1 shuffles once");
    assert_eq!(shuffles_of(outcome.events(), P0), 0, "P0 has no member");
}

/// Zero declared targets exile nothing, so no reveal runs at all.
#[test]
fn zero_filled_slots_is_a_clean_no_op() {
    let mut b = board();
    let outcome = b.runner.cast(b.spell).resolve();

    outcome.assert_zone(&[b.artifact, b.creature], Zone::Battlefield);
    outcome.assert_zone(&b.p0_library, Zone::Library);
    outcome.assert_zone(&b.p1_library, Zone::Library);
    assert_eq!(shuffles_of(outcome.events(), P0), 0);
    assert_eq!(shuffles_of(outcome.events(), P1), 0);
}
