//! Parser for Bloodborne PC port / recomp (ShadPS4 / bbport) achievements.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct TrophyMetadataFile {
    #[serde(default)]
    pub complete: Option<bool>,
    #[serde(default)]
    pub entries: Vec<TrophyMetadataEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrophyMetadataEntry {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub grade: String,
    #[serde(default)]
    pub score: Option<u32>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct BloodborneTrophy {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub grade: String,
    pub unlocked: bool,
    pub unlock_time: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct BloodborneScanResult {
    pub name: String,
    pub unlocked: u32,
    pub total: u32,
    pub pct: u32,
    pub platinum: bool,
    pub trophies: Vec<BloodborneTrophy>,
}

/// Parse Bloodborne trophies from the specified user directory.
/// Looks for:
/// - `RcloneUI/trophies.txt` or `trophies.txt` (unlocked IDs with timestamps)
/// - `trophy_cache/metadata.json` or `metadata.json` (trophy names, descriptions, grades)
pub fn parse_bloodborne_user_dir(user_dir: &Path) -> Result<BloodborneScanResult, String> {
    if !user_dir.exists() {
        return Err(format!("Directory '{}' does not exist", user_dir.display()));
    }

    // 1. Find and parse trophies.txt
    let candidate_trophies = [
        user_dir.join("RcloneUI").join("trophies.txt"),
        user_dir.join("trophies.txt"),
    ];

    let mut unlocked_map: HashMap<u32, u64> = HashMap::new();
    let mut found_trophies_file = false;

    for path in &candidate_trophies {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                for line in content.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let (Ok(id), Ok(ts)) = (parts[0].parse::<u32>(), parts[1].parse::<u64>()) {
                            unlocked_map.insert(id, ts);
                        }
                    } else if parts.len() == 1 {
                        if let Ok(id) = parts[0].parse::<u32>() {
                            unlocked_map.insert(id, 0);
                        }
                    }
                }
                found_trophies_file = true;
                break;
            }
        }
    }

    if !found_trophies_file {
        return Err(format!(
            "Could not find trophies.txt in '{}' or '{}/RcloneUI'",
            user_dir.display(),
            user_dir.display()
        ));
    }

    // 2. Find and parse metadata.json
    let candidate_metadata = [
        user_dir.join("trophy_cache").join("metadata.json"),
        user_dir.join("metadata.json"),
    ];

    let mut schema_entries: Vec<TrophyMetadataEntry> = Vec::new();

    for path in &candidate_metadata {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(parsed) = serde_json::from_str::<TrophyMetadataFile>(&content) {
                    schema_entries = parsed.entries;
                    break;
                }
            }
        }
    }

    // Fallback if metadata.json was missing: construct default 40 Bloodborne trophies
    if schema_entries.is_empty() {
        schema_entries = default_bloodborne_schema();
    }

    let mut trophies: Vec<BloodborneTrophy> = Vec::with_capacity(schema_entries.len());
    let mut unlocked_count = 0;
    let mut platinum_unlocked = false;

    for entry in schema_entries {
        let is_unlocked = unlocked_map.contains_key(&entry.id);
        let unlock_time = unlocked_map.get(&entry.id).copied();

        if is_unlocked {
            unlocked_count += 1;
            if entry.id == 0 || entry.grade.eq_ignore_ascii_case("P") {
                platinum_unlocked = true;
            }
        }

        trophies.push(BloodborneTrophy {
            id: entry.id,
            name: entry.name,
            description: entry.description,
            grade: entry.grade,
            unlocked: is_unlocked,
            unlock_time,
        });
    }

    let total = trophies.len() as u32;
    let pct = if total > 0 {
        (unlocked_count * 100) / total
    } else {
        0
    };

    if unlocked_count == total && total > 0 {
        platinum_unlocked = true;
    }

    Ok(BloodborneScanResult {
        name: "Bloodborne".to_string(),
        unlocked: unlocked_count,
        total,
        pct,
        platinum: platinum_unlocked,
        trophies,
    })
}

fn default_bloodborne_schema() -> Vec<TrophyMetadataEntry> {
    vec![
        TrophyMetadataEntry { id: 0, name: "Bloodborne".into(), description: "All trophies acquired. Hats off!".into(), grade: "P", score: Some(100), hidden: Some(false) },
        TrophyMetadataEntry { id: 1, name: "Yharnam Sunrise".into(), description: "You lived through the hunt, and saw another day.".into(), grade: "G", score: Some(60), hidden: Some(true) },
        TrophyMetadataEntry { id: 2, name: "Honoring Wishes".into(), description: "Captivated by the Moon Presence, you pledge to watch over the Hunter's Dream.".into(), grade: "G", score: Some(60), hidden: Some(true) },
        TrophyMetadataEntry { id: 3, name: "Childhood's Beginning".into(), description: "You became an infant Great One, lifting humanity into its next childhood.".into(), grade: "G", score: Some(60), hidden: Some(true) },
        TrophyMetadataEntry { id: 4, name: "Yharnam, Pthumerian Queen".into(), description: "Defeat Yharnam, Blood Queen of the Old Labyrinth.".into(), grade: "G", score: Some(60), hidden: Some(true) },
        TrophyMetadataEntry { id: 5, name: "Hunter's Essence".into(), description: "Acquire all hunter weapons.".into(), grade: "G", score: Some(60), hidden: Some(false) },
        TrophyMetadataEntry { id: 6, name: "Hunter's Craft".into(), description: "Acquire all special hunter tools.".into(), grade: "G", score: Some(60), hidden: Some(false) },
        TrophyMetadataEntry { id: 7, name: "Weapon Master".into(), description: "Acquire a weapon of the highest level.".into(), grade: "S", score: Some(30), hidden: Some(false) },
        TrophyMetadataEntry { id: 8, name: "Blood Gem Master".into(), description: "Acquire an extremely precious blood gem.".into(), grade: "S", score: Some(30), hidden: Some(false) },
        TrophyMetadataEntry { id: 9, name: "Rune Master".into(), description: "Acquire an extremely precious Caryll Rune.".into(), grade: "S", score: Some(30), hidden: Some(false) },
        TrophyMetadataEntry { id: 10, name: "Cainhurst".into(), description: "Gain entry to Cainhurst, the lost and ruined castle.".into(), grade: "S", score: Some(30), hidden: Some(true) },
        TrophyMetadataEntry { id: 11, name: "The Choir".into(), description: "Gain entry to the realm of the Choir, the high stratum of the Healing Church.".into(), grade: "S", score: Some(30), hidden: Some(true) },
        TrophyMetadataEntry { id: 12, name: "The Source of the Dream".into(), description: "Discover the abandoned old workshop, the source of the Hunter's Dream.".into(), grade: "S", score: Some(30), hidden: Some(true) },
        TrophyMetadataEntry { id: 13, name: "Nightmare Lecture Building".into(), description: "Gain entry into the Byrgenwerth lecture building.".into(), grade: "S", score: Some(30), hidden: Some(true) },
        TrophyMetadataEntry { id: 14, name: "Father Gascoigne".into(), description: "Defeat the beast that once was Father Gascoigne.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 15, name: "Vicar Amelia".into(), description: "Defeat the beast that once was Vicar Amelia.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 16, name: "Shadow of Yharnam".into(), description: "Defeat the Shadow of Yharnam.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 17, name: "Rom, the Vacuous Spider".into(), description: "Defeat Great One: Rom, the Vacuous Spider.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 18, name: "The One Reborn".into(), description: "Defeat The One Reborn.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 19, name: "Micolash, Host of the Nightmare".into(), description: "Defeat Micolash, Host of the Nightmare.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 20, name: "Mergo's Wet Nurse".into(), description: "Defeat Great One: Mergo's Wet Nurse.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 21, name: "Cleric Beast".into(), description: "Defeat Cleric Beast.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 22, name: "Blood-starved Beast".into(), description: "Defeat Blood-starved Beast.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 23, name: "The Witch of Hemwick".into(), description: "Defeat the Witch of Hemwick.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 24, name: "Darkbeast Paarl".into(), description: "Defeat Darkbeast Paarl.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 25, name: "Amygdala".into(), description: "Defeat Great One: Amygdala.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 26, name: "Martyr Logarius".into(), description: "Defeat Martyr Logarius.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 27, name: "Celestial Emissary".into(), description: "Defeat Great One: Celestial Emissary.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 28, name: "Ebrietas, Daughter of the Cosmos".into(), description: "Defeat Great One: Ebrietas, Daughter of the Cosmos.".into(), grade: "B", score: Some(15), hidden: Some(true) },
        TrophyMetadataEntry { id: 29, name: "Blood Gem Contact".into(), description: "Acquire a blood gem that imparts hunter weapons special strength.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 30, name: "Rune Contact".into(), description: "Acquire a Caryll Rune that imparts hunters special strength.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 31, name: "Chalice of Pthumeru".into(), description: "Acquire the Chalice of Pthumeru that seals the catacombs.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 32, name: "Chalice of Ailing Loran".into(), description: "Acquire the Chalice of Ailing Loran that seals the tragic land.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 33, name: "Chalice of Isz".into(), description: "Acquire the Great Chalice of Isz that seals the home of the cosmic kin.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 34, name: "Defiled Chalice".into(), description: "Acquire the Defiled Chalice that seals the cursed labyrinth.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 35, name: "Chalice of Great Pthumeru Ihyll".into(), description: "Acquire the Great Pthumeru Ihyll Chalice.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 36, name: "Chalice of Lower Pthumeru".into(), description: "Acquire the Lower Pthumeru Chalice.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 37, name: "Chalice of Hintertomb".into(), description: "Acquire the Hintertomb Chalice.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 38, name: "Chalice of Lower Hintertomb".into(), description: "Acquire the Lower Hintertomb Chalice.".into(), grade: "B", score: Some(15), hidden: Some(false) },
        TrophyMetadataEntry { id: 39, name: "Chalice of Lower Loran".into(), description: "Acquire the Lower Loran Chalice.".into(), grade: "B", score: Some(15), hidden: Some(false) },
    ]
}
