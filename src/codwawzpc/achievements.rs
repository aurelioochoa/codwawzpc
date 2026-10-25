/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/. */
//! The game's achievements, and detection of unlocks for the launcher.
//!
//! COD Zombies defines 106 achievements as IwResManager resources in
//! `_main.dz` (`fixed/achievements/*.group.bin`: one global group and one per
//! map). Each is a `CAchievementBase` subclass: most compare one of the
//! player's stats with a threshold (`CAchievementGreaterEqu` and friends);
//! `CAchievementDirect` ones are set from the levels' Lua scripts
//! (`SetAchievement`). Every frame the game checks each locked achievement
//! and, when it passes, appends its id to the save manager's list of
//! unlocked ids, shows "Achievement Unlocked" and plays its sound. The game
//! only does this in Regular mode; Recruit mode does not unlock them.
//!
//! [frame] reads that list, which also holds the unlocks loaded from the
//! game's save, and keeps `achievements.plist` in the data directory up to
//! date: the catalog below with each achievement's state, for the launcher.

use super::native::peek;
use crate::Environment;
use plist::{Dictionary, Value};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// `CSaveManager *`.
const SAVE_MANAGER: u32 = 0xa1a558;
const SAVE_MANAGER_VTABLE: u32 = 0x2a4e1c;
/// `CIwArray<int>` of unlocked achievement ids: data, count, capacity.
const UNLOCKED_IDS: u32 = 0x478;
/// `CGame *`; its state at +0x20 is 1 in the menus and 2 in a level. The
/// save has been loaded by then.
const GAME: u32 = 0xa15080;

pub const FILE_NAME: &str = "achievements.plist";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Map {
    Any,
    Nacht,
    Verruckt,
    ShiNoNuma,
    DerRiese,
}

impl Map {
    pub fn key(self) -> &'static str {
        match self {
            Map::Any => "any",
            Map::Nacht => "nacht",
            Map::Verruckt => "verruckt",
            Map::ShiNoNuma => "shinonuma",
            Map::DerRiese => "derriese",
        }
    }
}

pub struct Achievement {
    pub id: u32,
    pub points: u32,
    pub map: Map,
    pub name: &'static str,
    pub description: &'static str,
}

const fn a(
    id: u32,
    points: u32,
    map: Map,
    name: &'static str,
    description: &'static str,
) -> Achievement {
    Achievement {
        id,
        points,
        map,
        name,
        description,
    }
}

/// Every achievement of the supported release, by id (17 and 67 are
/// unused), with its English (US) name and description and the points shown
/// in the game. Recovered from `_main.dz`; see the module documentation.
pub const CATALOG: &[Achievement] = &[
    a(
        1,
        100,
        Map::Nacht,
        "Radio Silence?",
        "Hit the radio in Nacht der Untoten",
    ),
    a(2, 100, Map::Any, "My Flamin' Heart", "Get the Flamethrower"),
    a(
        3,
        500,
        Map::Any,
        "Smells like Chicken",
        "Roast 4 Zombies simultaneously",
    ),
    a(
        4,
        1000,
        Map::Any,
        "I, Pyromaniac",
        "Dish out 50,000 points of flamethrower damage",
    ),
    a(
        5,
        100,
        Map::Any,
        "Dramatic Exit",
        "Commit suicide with a grenade and take out 3 Zombies with you",
    ),
    a(
        6,
        300,
        Map::Nacht,
        "Hoarder",
        "Get to and complete round 5 using only 120 rounds of ammunition",
    ),
    a(
        7,
        300,
        Map::Nacht,
        "Pistols at Dawn",
        "Complete first 2 rounds using only the pistol and knife",
    ),
    a(
        8,
        100,
        Map::Nacht,
        "Du bist Wunderschön!",
        "Complete the tutorial without Richtofen shooting any Zombies for you",
    ),
    a(
        9,
        100,
        Map::Nacht,
        "Schnell, Schnell!",
        "Complete Wave 1 in under 60 seconds",
    ),
    a(
        10,
        100,
        Map::Any,
        "The Skull Collector",
        "Collect 10 Insta-kills",
    ),
    a(
        11,
        2000,
        Map::Any,
        "Severance",
        "Explode 2 Zombie heads simultaneously",
    ),
    a(
        12,
        1000,
        Map::Any,
        "Fresh From The Oven",
        "Kill 3 Zombies with a grenade that has been cooked for at least 2 seconds, without dying",
    ),
    a(
        13,
        1000,
        Map::Any,
        "Chastity",
        "Accumulate 2,000 points from repairing barricades",
    ),
    a(
        14,
        250,
        Map::Nacht,
        "No Barriers, No Fear",
        "Reach Wave 6 without repairing barriers",
    ),
    a(
        15,
        100,
        Map::Any,
        "Together, my brother!",
        "Join a friend in a Co-Op session",
    ),
    a(
        16,
        100,
        Map::Any,
        "Witching Hour",
        "Play the game during the \"Witching Hour\"",
    ),
    a(
        18,
        500,
        Map::Any,
        "Munitions Expert I",
        "Fire 1000 rounds of ammunition (overall)",
    ),
    a(
        19,
        1000,
        Map::Any,
        "Munitions Expert II",
        "Fire 4000 rounds of ammunition (overall)",
    ),
    a(
        20,
        2000,
        Map::Any,
        "Munitions Expert III",
        "Fire 10,000 rounds of ammunition (overall)",
    ),
    a(21, 500, Map::Any, "The Grinder I", "Gib 50 enemies"),
    a(22, 1000, Map::Any, "The Grinder II", "Gib 200 enemies"),
    a(23, 2000, Map::Any, "The Grinder III", "Gib 400 enemies"),
    a(24, 500, Map::Any, "Poppin' Heads I", "Pop 25 Zombie heads"),
    a(
        25,
        1000,
        Map::Any,
        "Poppin' Heads II",
        "Pop 100 Zombie heads",
    ),
    a(
        26,
        5000,
        Map::Any,
        "Poppin' Heads III",
        "Pop 250 Zombie heads",
    ),
    a(
        27,
        2000,
        Map::Any,
        "Armless...",
        "Remove both arms from 25 Zombies",
    ),
    a(
        28,
        2000,
        Map::Any,
        "...Legless",
        "Remove the legs from 35 Zombies",
    ),
    a(
        29,
        2000,
        Map::Any,
        "Eagle Eye",
        "Kill a Zombie from 50 ft away",
    ),
    a(
        30,
        1000,
        Map::Any,
        "Running Man",
        "Cover the distance of 10 miles",
    ),
    a(
        31,
        500,
        Map::Nacht,
        "Blow Me Away",
        "Kill 20 Zombies with exploding barrels",
    ),
    a(
        32,
        1000,
        Map::Nacht,
        "Stay off my Lawn!",
        "Survive 2 waves without letting a single Zombie into the building in Nacht der Untoten",
    ),
    a(
        33,
        1000,
        Map::Any,
        "Nuclear Lollercaust",
        "Collect 10 nuke pickups",
    ),
    a(
        34,
        1000,
        Map::Any,
        "Double Down",
        "Collect 10 double-points pickups",
    ),
    a(
        35,
        100,
        Map::Any,
        "High five, bro!",
        "Knife your Co-Op partner in the face",
    ),
    a(
        36,
        100,
        Map::Any,
        "Take My Hand",
        "Revive a downed Co-Op partner",
    ),
    a(
        37,
        1000,
        Map::Any,
        "The Good Samaritan",
        "Revive downed allies 10 times",
    ),
    a(
        38,
        2000,
        Map::Any,
        "Laid Back Executioner",
        "Kill 5 Zombies when downed in Co-Op mode",
    ),
    a(
        39,
        1000,
        Map::Nacht,
        "Hard-ass",
        "Take no damage for 5 rounds",
    ),
    a(
        40,
        1000,
        Map::Nacht,
        "This is my Boomstick!",
        "Survive 5 waves using shotguns",
    ),
    a(
        41,
        1000,
        Map::Any,
        "I Like Those Odds",
        "Use the mystery box 20 times",
    ),
    a(
        42,
        100,
        Map::Any,
        "Space Man!",
        "Receive the Ray Gun from the mystery box",
    ),
    a(
        43,
        3000,
        Map::Any,
        "Laser Face",
        "Kill 100 Zombies with the Ray Gun",
    ),
    a(
        44,
        5000,
        Map::Any,
        "Eternal Investments",
        "Spend 50,000 points (overall)",
    ),
    a(
        45,
        1000,
        Map::Nacht,
        "OK, I think I can do this...",
        "Reach Wave 5",
    ),
    a(46, 3000, Map::Nacht, "I can make it!", "Reach Wave 12"),
    a(47, 5000, Map::Nacht, "Awesome Badassery", "Reach Wave 20"),
    a(
        48,
        100,
        Map::Verruckt,
        "Open Wide...",
        "Activate the dentists chair",
    ),
    a(
        49,
        250,
        Map::Any,
        "Hyperactive",
        "Have drank 10 Revive Sodas",
    ),
    a(
        50,
        250,
        Map::Any,
        "Tickets to the Gun Show",
        "Have drank 10 Jugger-nogs",
    ),
    a(
        51,
        250,
        Map::Any,
        "When Can I Tap That",
        "Have drank 10 Double Tap Root Beers",
    ),
    a(
        52,
        250,
        Map::Any,
        "Palm Me Off",
        "Have drank 10 Speed Colas",
    ),
    a(
        53,
        500,
        Map::Verruckt,
        "Electro-Shock Therapy",
        "Have killed 50 zombies with Electro-Shock Barriers",
    ),
    a(
        54,
        100,
        Map::Verruckt,
        "You Got the Powah!",
        "Activate the generator",
    ),
    a(
        55,
        100,
        Map::Any,
        "Miracle Hands",
        "Revive a Co-Op partner using Revive Soda",
    ),
    a(
        56,
        500,
        Map::Any,
        "This is for you, Betty!",
        "Kill 2 zombies at once with a single Bouncing Betty",
    ),
    a(
        57,
        1000,
        Map::Verruckt,
        "I'm Buzzing Baby!",
        "Have all perk-a colas at once",
    ),
    a(
        58,
        500,
        Map::Verruckt,
        "Das Ist Mein Haus",
        "Opened up all the asylum areas",
    ),
    a(
        59,
        1000,
        Map::Verruckt,
        "Pass Me A Straightjacket",
        "Reach Wave 5 in Verrückt",
    ),
    a(
        60,
        3000,
        Map::Verruckt,
        "Lock Me Up...",
        "Reach Wave 12 in Verrückt",
    ),
    a(
        61,
        5000,
        Map::Verruckt,
        "Throw Away The Key",
        "Reach Wave 20 in Verrückt",
    ),
    a(
        62,
        250,
        Map::Verruckt,
        "And Stay Down!",
        "Kill a zombie as it emerges from the ground",
    ),
    a(
        63,
        1000,
        Map::Verruckt,
        "My Kind Of Psychiatry",
        "Survive 5 waves using only the flamethrower in Verrückt",
    ),
    a(
        64,
        500,
        Map::Any,
        "Feel My Blade",
        "Kill 15 zombies in a row with only a knife without dying",
    ),
    a(
        65,
        1000,
        Map::Any,
        "I Think I Need Help",
        "Kill 30 zombies in a row with only a knife without dying",
    ),
    a(
        66,
        250,
        Map::Any,
        "The Coldest Steel",
        "Kill 100 zombies overall with the knife",
    ),
    a(
        68,
        1000,
        Map::Any,
        "oneShotRod",
        "Pwn the Brainless with your boomstick (kill 4 zombies with one shotgun shot)",
    ),
    a(69, 100, Map::ShiNoNuma, "Dead Air", "Activate the phone"),
    a(
        70,
        1000,
        Map::Any,
        "Face Melter",
        "Electrocute 8 zombies with one Wunderwaffe shot",
    ),
    a(
        71,
        250,
        Map::Any,
        "Weapon of Minor Damage",
        "Kill only one zombie with a Nuke",
    ),
    a(
        72,
        1000,
        Map::Any,
        "Big Baller",
        "Earn 20,000 points in a single game",
    ),
    a(
        73,
        1000,
        Map::Any,
        "Big Brawler",
        "Kill 30 zombies in a row with only a magnum",
    ),
    a(
        74,
        100,
        Map::Any,
        "On yer 'Ed",
        "Kill a zombie with a grenade that you bounced off a team-mate",
    ),
    a(
        75,
        250,
        Map::ShiNoNuma,
        "It's a Trap!",
        "Kill at least 1 zombie with the flogger and an electro-barrier",
    ),
    a(
        76,
        500,
        Map::ShiNoNuma,
        "Fertilizer Man",
        "Kill 200 zombies in the Shi No Numa map",
    ),
    a(
        77,
        500,
        Map::ShiNoNuma,
        "Deadhead",
        "Kill 150 zombies by head shots in Shi No Numa",
    ),
    a(
        78,
        100,
        Map::Any,
        "Hong Kong Akimbo",
        "Have both pistols at once",
    ),
    a(
        79,
        500,
        Map::ShiNoNuma,
        "Ew, my shoes are ruined!",
        "Opened up all the Shi No Numa areas",
    ),
    a(
        80,
        1000,
        Map::ShiNoNuma,
        "Waterlogged",
        "Reach Wave 5 in Shi No Numa",
    ),
    a(
        81,
        3000,
        Map::ShiNoNuma,
        "Bogged Down",
        "Reach Wave 12 in Shi No Numa",
    ),
    a(
        82,
        5000,
        Map::ShiNoNuma,
        "Totally Swamped",
        "Reach Wave 20 in Shi No Numa",
    ),
    a(
        83,
        250,
        Map::ShiNoNuma,
        "And Stay Drown!",
        "Kill a zombie as they burst out of a swamp",
    ),
    a(
        84,
        500,
        Map::ShiNoNuma,
        "Soul Survivor",
        "Reach wave 15 in Shi No Numa without going into Last Stand (Multiplayer)",
    ),
    a(
        85,
        100,
        Map::ShiNoNuma,
        "Flagellater",
        "Kill 10 zombies in one bout of the Flogger",
    ),
    a(86, 250, Map::Any, "Play dead boy!", "Kill 50 Hellhounds"),
    a(
        87,
        500,
        Map::Any,
        "Hung, Drawn...",
        "Shoot off a zombies arms and then blow their legs off",
    ),
    a(
        88,
        1000,
        Map::Any,
        "... and Quartered",
        "Shoot off a zombies arms, blow their legs off and then shoot their head off",
    ),
    a(
        89,
        500,
        Map::DerRiese,
        "Elevate Your Senses",
        "Activate the Fly Trap",
    ),
    a(
        90,
        250,
        Map::DerRiese,
        "Locksmith",
        "Open all the doors in the Der Riese (in one game)",
    ),
    a(
        91,
        500,
        Map::DerRiese,
        "Der Electrician",
        "Link all of the teleport pads before Round 7",
    ),
    a(
        92,
        1000,
        Map::DerRiese,
        "Frequent Flyer",
        "Use the teleporter 8 times(in one game)",
    ),
    a(
        93,
        5000,
        Map::DerRiese,
        "Pack Addict",
        "Upgrade 5 weapons at the Pack-a-Punch Machine (in one game)",
    ),
    a(
        94,
        250,
        Map::DerRiese,
        "The Poetry of Steak",
        "Killed 10 zombies with the teleporter",
    ),
    a(95, 250, Map::DerRiese, "Mein Shiny", "Upgraded a weapon"),
    a(
        96,
        1000,
        Map::DerRiese,
        "The Touch of Gott",
        "Have 2 pack-a-punched weapons at once",
    ),
    a(
        97,
        500,
        Map::DerRiese,
        "Standing Tall",
        "Reach Wave 10 in Der Riese",
    ),
    a(
        98,
        1000,
        Map::DerRiese,
        "Against Goliath",
        "Reach Wave 20 in Der Riese",
    ),
    a(
        99,
        3000,
        Map::DerRiese,
        "On the Shoulder Of Giants",
        "Reach Wave 30 in Der Riese",
    ),
    a(
        100,
        500,
        Map::DerRiese,
        "Wurst Case",
        "Kill 40 zombies using the Bowie knife (in one game)",
    ),
    a(
        101,
        250,
        Map::Any,
        "40 Knives",
        "Kill 40 zombies using the Bowie knife (in one game)",
    ),
    a(
        102,
        100,
        Map::Any,
        "The Might of the Monkey",
        "Use the Monkey Bomb",
    ),
    a(
        103,
        500,
        Map::Any,
        "Perkaholics Anonymous",
        "Survive until Round 20 without buying a single perk",
    ),
    a(
        104,
        1000,
        Map::Any,
        "Acquire Waffle Weapons",
        "Obtain the Ray Gun, Wunderwaffe DG-2 and the Monkey Bomb at the same time",
    ),
    a(
        105,
        100,
        Map::Any,
        "It's Tool Time",
        "Used the carpenter's hammer five times (accumulatively)",
    ),
    a(
        106,
        100,
        Map::Any,
        "Simian Justice",
        "Killed 20 zombies or dogs using Monkey Bombs",
    ),
    a(
        107,
        100,
        Map::DerRiese,
        "And Stay ... Up?",
        "Kill a zombie climbing up a ledge",
    ),
    a(
        108,
        1000,
        Map::Any,
        "Wunderkind",
        "Completed all easter eggs, used all wunder-weapons (in game)",
    ),
];

pub fn find(id: u32) -> Option<&'static Achievement> {
    CATALOG.iter().find(|a| a.id == id)
}

/// Unlocked ids and when codwawzpc first saw each (Unix seconds).
type Unlocked = BTreeMap<u32, u64>;

struct State {
    path: std::path::PathBuf,
    unlocked: Unlocked,
    /// The game's list at the last check: (data pointer, count).
    last: Option<(u32, u32)>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Check the game's unlocked list. Called once per presented frame; reads
/// three words unless the list changed.
pub(crate) fn frame(env: &mut Environment) {
    let Some(config) = super::runtime::CONFIG.get() else {
        return;
    };
    let Some(list) = unlocked_list(env) else {
        return;
    };
    let mut guard = STATE.lock().unwrap();
    let state = guard.get_or_insert_with(|| {
        let path = config.data_directory.join(FILE_NAME);
        let unlocked = read_unlocked(&path).unwrap_or_default();
        State {
            path,
            unlocked,
            last: None,
        }
    });
    if state.last == Some(list) {
        return;
    }
    // The first check sees what the game loaded from its save.
    let from_save = state.last.is_none();
    state.last = Some(list);
    let (data, count) = list;
    let ids: Vec<u32> = (0..count).filter_map(|i| peek(env, data + i * 4)).collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Follow the save: a reset save starts over.
    let mut unlocked = Unlocked::new();
    let mut newly_earned = Vec::new();
    for &id in &ids {
        let first_seen = state.unlocked.get(&id).copied();
        if first_seen.is_none() {
            let how = if from_save { "in save" } else { "unlocked" };
            if let Some(a) = find(id) {
                log!(
                    "Achievement {}: {} \"{}\" ({})",
                    how,
                    id,
                    a.name,
                    a.description
                );
                if !from_save {
                    newly_earned.push(id);
                }
            } else {
                log!("Achievement {}: unknown id {}", how, id);
            }
        }
        unlocked.insert(id, first_seen.unwrap_or(now));
    }
    for id in newly_earned {
        if let Some(window) = env.window.as_ref() {
            window.request_achievement_capture(id);
        }
    }
    if from_save {
        log!("Achievements: {} of {} unlocked", ids.len(), CATALOG.len());
    }
    if unlocked != state.unlocked || !state.path.exists() {
        state.unlocked = unlocked;
        if let Err(e) = write(&state.path, &state.unlocked) {
            log!("Cannot write {}: {}", state.path.display(), e);
        }
    }
}

/// The save manager's unlocked list as (data pointer, count), once the game
/// has loaded its save.
fn unlocked_list(env: &Environment) -> Option<(u32, u32)> {
    let game = peek(env, GAME).filter(|&g| g != 0)?;
    if !matches!(peek(env, game + 0x20)?, 1 | 2) {
        return None;
    }
    let manager =
        peek(env, SAVE_MANAGER).filter(|&m| m != 0 && peek(env, m) == Some(SAVE_MANAGER_VTABLE))?;
    let data = peek(env, manager + UNLOCKED_IDS)?;
    let count = peek(env, manager + UNLOCKED_IDS + 4)?;
    (count <= 256 && (count == 0 || data != 0)).then_some((data, count))
}

/// `achievements.plist`: format version, totals, and every achievement with
/// its id, name, description, points, map, whether it is unlocked and, if
/// so, when codwawzpc first saw it unlocked (`UnlockedAt`, Unix seconds).
pub fn to_plist(unlocked: &Unlocked) -> Value {
    let mut list = Vec::new();
    for a in CATALOG {
        let mut d = Dictionary::new();
        d.insert("Id".into(), Value::Integer(a.id.into()));
        d.insert("Name".into(), Value::String(a.name.into()));
        d.insert("Description".into(), Value::String(a.description.into()));
        d.insert("Points".into(), Value::Integer(a.points.into()));
        d.insert("Map".into(), Value::String(a.map.key().into()));
        d.insert(
            "Unlocked".into(),
            Value::Boolean(unlocked.contains_key(&a.id)),
        );
        if let Some(&at) = unlocked.get(&a.id) {
            d.insert("UnlockedAt".into(), Value::Integer(at.into()));
        }
        list.push(Value::Dictionary(d));
    }
    let earned: u32 = CATALOG
        .iter()
        .filter(|a| unlocked.contains_key(&a.id))
        .map(|a| a.points)
        .sum();
    let mut d = Dictionary::new();
    d.insert("Format".into(), Value::Integer(1.into()));
    d.insert(
        "Total".into(),
        Value::Integer((CATALOG.len() as u64).into()),
    );
    d.insert(
        "UnlockedCount".into(),
        Value::Integer((unlocked.len() as u64).into()),
    );
    d.insert(
        "TotalPoints".into(),
        Value::Integer(
            CATALOG
                .iter()
                .map(|a| u64::from(a.points))
                .sum::<u64>()
                .into(),
        ),
    );
    d.insert("EarnedPoints".into(), Value::Integer(earned.into()));
    d.insert("Achievements".into(), Value::Array(list));
    Value::Dictionary(d)
}

fn write(path: &std::path::Path, unlocked: &Unlocked) -> Result<(), String> {
    super::import::write_plist_atomic(path, &to_plist(unlocked))
}

/// The unlocked ids recorded in `achievements.plist`, if it exists.
pub fn read_unlocked(path: &std::path::Path) -> Option<Unlocked> {
    let value = Value::from_file(path).ok()?;
    let list = value.as_dictionary()?.get("Achievements")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|v| {
                let d = v.as_dictionary()?;
                if !d.get("Unlocked")?.as_boolean()? {
                    return None;
                }
                let id = u32::try_from(d.get("Id")?.as_unsigned_integer()?).ok()?;
                let at = d
                    .get("UnlockedAt")
                    .and_then(|v| v.as_unsigned_integer())
                    .unwrap_or(0);
                Some((id, at))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_complete_and_consistent() {
        assert_eq!(CATALOG.len(), 106);
        assert_eq!(CATALOG.iter().map(|a| a.points).sum::<u32>(), 105_050);
        let mut ids: Vec<u32> = CATALOG.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 106);
        assert_eq!(
            ids,
            (1..=108)
                .filter(|&i| i != 17 && i != 67)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn plist_round_trip() {
        let p = std::env::temp_dir().join(format!("codwawzpc-ach-{}", uuid::Uuid::new_v4()));
        let path = p.join(FILE_NAME);
        let unlocked: Unlocked = [(1, 100), (45, 200)].into();
        write(&path, &unlocked).unwrap();
        assert_eq!(read_unlocked(&path).unwrap(), unlocked);
        let d = Value::from_file(&path).unwrap().into_dictionary().unwrap();
        assert_eq!(d["UnlockedCount"].as_unsigned_integer(), Some(2));
        assert_eq!(d["EarnedPoints"].as_unsigned_integer(), Some(1100));
        std::fs::remove_dir_all(p).unwrap();
    }
}
