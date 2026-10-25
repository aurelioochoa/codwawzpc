# Achievements

COD Zombies 1.5.0 has 106 achievements worth 105,050 points. They are part
of the game's data, not an online service, and unlock in solo play (a few
need Co-Op, which is not available offline). This
page describes how the game handles them, how codwawzpc reports them, and
lists them all.

## How the game handles them

- **Definitions.** `_main.dz` holds five IwResManager groups in
  `fixed/achievements/`: `achievements` (59 available on any map) and one
  per map (`nacht_`, `verruckt_`, `shinonuma_`, `derriese_achievements`).
  Each achievement is a serialized `CAchievementBase` subclass with an id,
  points, the localisation hashes of its name and description, and its
  condition. Strings are looked up in `localisation/LocaliseHashes.dat` and
  `english_us.dat`; hashes are Marmalade's `IwHashString` (djb2 of the
  lowercased string).
- **Conditions.** Most compare one or more of the player's stats with a
  threshold (`CAchievementGreaterEqu` and its variants). Stats live in the
  save manager (`CSaveManager`, `*0xa1a558`), grouped per map. `Direct`
  achievements (hitting the radio, the dentist's chair, the generator, …)
  are set from the levels' Lua scripts with `SetAchievement`. Some also need
  other achievements first.
- **Unlocking.** Every frame the game checks each locked achievement. When
  one passes, it appends the id to the save manager's unlocked list
  (`CIwArray<int>` at `CSaveManager + 0x478`), shows "Achievement Unlocked"
  with the name, plays the `achievement` sound and saves. Achievements only
  unlock in Regular mode; Recruit mode does not save stats or unlock them.

## What codwawzpc reports

Once per frame, codwawzpc reads the unlocked list (three words unless it
changed). The first read after the game loads its save covers earlier
unlocks too. Every change is written to `achievements.plist` in the data
directory (`$XDG_DATA_HOME/codwawzpc`, by default
`~/.local/share/codwawzpc`), an XML property list:

| Key | Type | Meaning |
| --- | --- | --- |
| `Format` | integer | 1 |
| `Total`, `UnlockedCount` | integer | Achievements in the game, and unlocked |
| `TotalPoints`, `EarnedPoints` | integer | Points in the game, and earned |
| `Achievements` | array | One dictionary per achievement, by id |

Each achievement has `Id`, `Name`, `Description`, `Points`, `Map` (`any`,
`nacht`, `verruckt`, `shinonuma` or `derriese`) and `Unlocked`. Unlocked
achievements also have `UnlockedAt`: when codwawzpc first saw them, in Unix
seconds. That is the unlock time for achievements unlocked while codwawzpc
was recording, and the first launch with this feature for ones already in
the save. The file follows the game's save, so a reset save resets it too.
The log also records each unlock (`Achievement unlocked: …`).

The launcher has one individually image-generated trophy for each achievement,
inspired by the original console trophy artwork and World at War concept art.
The [artwork notes](../launcher/artwork/achievement-artwork.md) record the
references, masters and generation prompts. Optional in-game media is
captured when a new unlock happens during play: a screenshot by default, and
a short video clip when enabled in Settings → Achievement captures or with the
Achievements screen toggles. Screenshots default to lossless PNG at the full
rendered window size. Clips default to H.264 at 60 FPS, up to 1080p, with High
quality and roughly three seconds on either side of the unlock. Clips require
FFmpeg with `libx264`. Settings → Achievement captures also controls screenshot
format/resolution and clip frame rate, resolution and compression quality;
see the [capture preferences](../launcher/README.md). Encoding runs in the
background with a compressed rolling buffer. Media is stored under
`$XDG_DATA_HOME/codwawzpc/achievement-media/<id>/`. Past unlocks cannot be
reconstructed from the save or `achievements.plist`; they need to be earned
again in a future game session to have real screenshots and clips.

Tested end to end on Nacht der Untoten: raising the insta-kill stat to 10
made the game unlock "The Skull Collector" itself, with its banner. The
unlock was in the save and in `achievements.plist` after a restart.

## List

| Id | Name | Description | Points | Map | Condition |
| ---: | --- | --- | ---: | --- | --- |
| 1 | Radio Silence? | Hit the radio in Nacht der Untoten | 100 | Nacht der Untoten | scripted |
| 2 | My Flamin' Heart | Get the Flamethrower | 100 | Any | stat ≥ |
| 3 | Smells like Chicken | Roast 4 Zombies simultaneously | 500 | Any | stat ≥ |
| 4 | I, Pyromaniac | Dish out 50,000 points of flamethrower damage | 1,000 | Any | stat ≥ |
| 5 | Dramatic Exit | Commit suicide with a grenade and take out 3 Zombies with you | 100 | Any | two stats |
| 6 | Hoarder | Get to and complete round 5 using only 120 rounds of ammunition | 300 | Nacht der Untoten | two stats |
| 7 | Pistols at Dawn | Complete first 2 rounds using only the pistol and knife | 300 | Nacht der Untoten | weapon |
| 8 | Du bist Wunderschön! | Complete the tutorial without Richtofen shooting any Zombies for you | 100 | Nacht der Untoten | scripted |
| 9 | Schnell, Schnell! | Complete Wave 1 in under 60 seconds | 100 | Nacht der Untoten | two stats |
| 10 | The Skull Collector | Collect 10 Insta-kills | 100 | Any | stat ≥ |
| 11 | Severance | Explode 2 Zombie heads simultaneously | 2,000 | Any | stat ≥ |
| 12 | Fresh From The Oven | Kill 3 Zombies with a grenade that has been cooked for at least 2 seconds, without dying | 1,000 | Any | three stats |
| 13 | Chastity | Accumulate 2,000 points from repairing barricades | 1,000 | Any | stat ≥ |
| 14 | No Barriers, No Fear | Reach Wave 6 without repairing barriers | 250 | Nacht der Untoten | two stats |
| 15 | Together, my brother! | Join a friend in a Co-Op session | 100 | Any | stat ≥ |
| 16 | Witching Hour | Play the game during the "Witching Hour" | 100 | Any | stat = |
| 18 | Munitions Expert I | Fire 1000 rounds of ammunition (overall) | 500 | Any | stat ≥ |
| 19 | Munitions Expert II | Fire 4000 rounds of ammunition (overall) | 1,000 | Any | stat ≥ |
| 20 | Munitions Expert III | Fire 10,000 rounds of ammunition (overall) | 2,000 | Any | stat ≥ |
| 21 | The Grinder I | Gib 50 enemies | 500 | Any | stat ≥ |
| 22 | The Grinder II | Gib 200 enemies | 1,000 | Any | stat ≥ |
| 23 | The Grinder III | Gib 400 enemies | 2,000 | Any | stat ≥ |
| 24 | Poppin' Heads I | Pop 25 Zombie heads | 500 | Any | stat ≥ |
| 25 | Poppin' Heads II | Pop 100 Zombie heads | 1,000 | Any | stat ≥ |
| 26 | Poppin' Heads III | Pop 250 Zombie heads | 5,000 | Any | stat ≥ |
| 27 | Armless... | Remove both arms from 25 Zombies | 2,000 | Any | stat ≥ |
| 28 | ...Legless | Remove the legs from 35 Zombies | 2,000 | Any | stat ≥ |
| 29 | Eagle Eye | Kill a Zombie from 50 ft away | 2,000 | Any | stat ≥ |
| 30 | Running Man | Cover the distance of 10 miles | 1,000 | Any | stat ≥ |
| 31 | Blow Me Away | Kill 20 Zombies with exploding barrels | 500 | Nacht der Untoten | stat ≥ |
| 32 | Stay off my Lawn! | Survive 2 waves without letting a single Zombie into the building in Nacht der Untoten | 1,000 | Nacht der Untoten | stat ≥ |
| 33 | Nuclear Lollercaust | Collect 10 nuke pickups | 1,000 | Any | stat ≥ |
| 34 | Double Down | Collect 10 double-points pickups | 1,000 | Any | stat ≥ |
| 35 | High five, bro! | Knife your Co-Op partner in the face | 100 | Any | stat ≥ |
| 36 | Take My Hand | Revive a downed Co-Op partner | 100 | Any | stat ≥ |
| 37 | The Good Samaritan | Revive downed allies 10 times | 1,000 | Any | stat ≥ |
| 38 | Laid Back Executioner | Kill 5 Zombies when downed in Co-Op mode | 2,000 | Any | stat ≥ |
| 39 | Hard-ass | Take no damage for 5 rounds | 1,000 | Nacht der Untoten | stat ≥ |
| 40 | This is my Boomstick! | Survive 5 waves using shotguns | 1,000 | Nacht der Untoten | weapon |
| 41 | I Like Those Odds | Use the mystery box 20 times | 1,000 | Any | stat ≥ |
| 42 | Space Man! | Receive the Ray Gun from the mystery box | 100 | Any | stat ≥ |
| 43 | Laser Face | Kill 100 Zombies with the Ray Gun | 3,000 | Any | stat ≥ |
| 44 | Eternal Investments | Spend 50,000 points (overall) | 5,000 | Any | stat ≥ |
| 45 | OK, I think I can do this... | Reach Wave 5 | 1,000 | Nacht der Untoten | stat ≥ |
| 46 | I can make it! | Reach Wave 12 | 3,000 | Nacht der Untoten | stat ≥ |
| 47 | Awesome Badassery | Reach Wave 20 | 5,000 | Nacht der Untoten | stat ≥ |
| 48 | Open Wide... | Activate the dentists chair | 100 | Verrückt | scripted |
| 49 | Hyperactive | Have drank 10 Revive Sodas | 250 | Any | stat ≥ |
| 50 | Tickets to the Gun Show | Have drank 10 Jugger-nogs | 250 | Any | stat ≥ |
| 51 | When Can I Tap That | Have drank 10 Double Tap Root Beers | 250 | Any | stat ≥ |
| 52 | Palm Me Off | Have drank 10 Speed Colas | 250 | Any | stat ≥ |
| 53 | Electro-Shock Therapy | Have killed 50 zombies with Electro-Shock Barriers | 500 | Verrückt | stat ≥ |
| 54 | You Got the Powah! | Activate the generator | 100 | Verrückt | scripted |
| 55 | Miracle Hands | Revive a Co-Op partner using Revive Soda | 100 | Any | stat ≥ |
| 56 | This is for you, Betty! | Kill 2 zombies at once with a single Bouncing Betty | 500 | Any | stat ≥ |
| 57 | I'm Buzzing Baby! | Have all perk-a colas at once | 1,000 | Verrückt | stat ≥ |
| 58 | Das Ist Mein Haus | Opened up all the asylum areas | 500 | Verrückt | stat ≥ |
| 59 | Pass Me A Straightjacket | Reach Wave 5 in Verrückt | 1,000 | Verrückt | stat ≥ |
| 60 | Lock Me Up... | Reach Wave 12 in Verrückt | 3,000 | Verrückt | stat ≥ |
| 61 | Throw Away The Key | Reach Wave 20 in Verrückt | 5,000 | Verrückt | stat ≥ |
| 62 | And Stay Down! | Kill a zombie as it emerges from the ground | 250 | Verrückt | stat ≥ |
| 63 | My Kind Of Psychiatry | Survive 5 waves using only the flamethrower in Verrückt | 1,000 | Verrückt | weapon |
| 64 | Feel My Blade | Kill 15 zombies in a row with only a knife without dying | 500 | Any | kills in a row |
| 65 | I Think I Need Help | Kill 30 zombies in a row with only a knife without dying | 1,000 | Any | kills in a row |
| 66 | The Coldest Steel | Kill 100 zombies overall with the knife | 250 | Any | stat ≥ |
| 68 | oneShotRod | Pwn the Brainless with your boomstick (kill 4 zombies with one shotgun shot) | 1,000 | Any | stat ≥ |
| 69 | Dead Air | Activate the phone | 100 | Shi No Numa | scripted |
| 70 | Face Melter | Electrocute 8 zombies with one Wunderwaffe shot | 1,000 | Any | stat ≥ |
| 71 | Weapon of Minor Damage | Kill only one zombie with a Nuke | 250 | Any | stat = |
| 72 | Big Baller | Earn 20,000 points in a single game | 1,000 | Any | stat ≥ |
| 73 | Big Brawler | Kill 30 zombies in a row with only a magnum | 1,000 | Any | kills in a row |
| 74 | On yer 'Ed | Kill a zombie with a grenade that you bounced off a team-mate | 100 | Any | stat ≥ |
| 75 | It's a Trap! | Kill at least 1 zombie with the flogger and an electro-barrier | 250 | Shi No Numa | two stats |
| 76 | Fertilizer Man | Kill 200 zombies in the Shi No Numa map | 500 | Shi No Numa | stat ≥ |
| 77 | Deadhead | Kill 150 zombies by head shots in Shi No Numa | 500 | Shi No Numa | stat ≥ |
| 78 | Hong Kong Akimbo | Have both pistols at once | 100 | Any | inventory |
| 79 | Ew, my shoes are ruined! | Opened up all the Shi No Numa areas | 500 | Shi No Numa | stat ≥ |
| 80 | Waterlogged | Reach Wave 5 in Shi No Numa | 1,000 | Shi No Numa | stat ≥ |
| 81 | Bogged Down | Reach Wave 12 in Shi No Numa | 3,000 | Shi No Numa | stat ≥ |
| 82 | Totally Swamped | Reach Wave 20 in Shi No Numa | 5,000 | Shi No Numa | stat ≥ |
| 83 | And Stay Drown! | Kill a zombie as they burst out of a swamp | 250 | Shi No Numa | stat ≥ |
| 84 | Soul Survivor | Reach wave 15 in Shi No Numa without going into Last Stand (Multiplayer) | 500 | Shi No Numa | two stats |
| 85 | Flagellater | Kill 10 zombies in one bout of the Flogger | 100 | Shi No Numa | stat ≥ |
| 86 | Play dead boy! | Kill 50 Hellhounds | 250 | Any | stat ≥ |
| 87 | Hung, Drawn... | Shoot off a zombies arms and then blow their legs off | 500 | Any | stat ≥ |
| 88 | ... and Quartered | Shoot off a zombies arms, blow their legs off and then shoot their head off | 1,000 | Any | stat ≥ |
| 89 | Elevate Your Senses | Activate the Fly Trap | 500 | Der Riese | scripted |
| 90 | Locksmith | Open all the doors in the Der Riese (in one game) | 250 | Der Riese | stat ≥ |
| 91 | Der Electrician | Link all of the teleport pads before Round 7 | 500 | Der Riese | two stats |
| 92 | Frequent Flyer | Use the teleporter 8 times(in one game) | 1,000 | Der Riese | stat ≥ |
| 93 | Pack Addict | Upgrade 5 weapons at the Pack-a-Punch Machine (in one game) | 5,000 | Der Riese | stat ≥ |
| 94 | The Poetry of Steak | Killed 10 zombies with the teleporter | 250 | Der Riese | stat ≥ |
| 95 | Mein Shiny | Upgraded a weapon | 250 | Der Riese | stat ≥ |
| 96 | The Touch of Gott | Have 2 pack-a-punched weapons at once | 1,000 | Der Riese | scripted |
| 97 | Standing Tall | Reach Wave 10 in Der Riese | 500 | Der Riese | stat ≥ |
| 98 | Against Goliath | Reach Wave 20 in Der Riese | 1,000 | Der Riese | stat ≥ |
| 99 | On the Shoulder Of Giants | Reach Wave 30 in Der Riese | 3,000 | Der Riese | stat ≥ |
| 100 | Wurst Case | Kill 40 zombies using the Bowie knife (in one game) | 500 | Der Riese | stat ≥ |
| 101 | 40 Knives | Kill 40 zombies using the Bowie knife (in one game) | 250 | Any | stat ≥ |
| 102 | The Might of the Monkey | Use the Monkey Bomb | 100 | Any | scripted |
| 103 | Perkaholics Anonymous | Survive until Round 20 without buying a single perk | 500 | Any | two stats |
| 104 | Acquire Waffle Weapons | Obtain the Ray Gun, Wunderwaffe DG-2 and the Monkey Bomb at the same time | 1,000 | Any | scripted |
| 105 | It's Tool Time | Used the carpenter's hammer five times (accumulatively) | 100 | Any | stat ≥ |
| 106 | Simian Justice | Killed 20 zombies or dogs using Monkey Bombs | 100 | Any | sum of two stats |
| 107 | And Stay ... Up? | Kill a zombie climbing up a ledge | 100 | Der Riese | stat ≥ |
| 108 | Wunderkind | Completed all easter eggs, used all wunder-weapons (in game) | 1,000 | Any | all wonder weapons |
