// Follows a League of Legends match through the game's own Live Client
// Data API: while a match is running the game serves
// https://127.0.0.1:2999/liveclientdata/allgamedata (a self-signed
// certificate, no key or login needed; Riot made it for tools like this).
// It gives the mode and map, and every champion kill with the killer,
// victim and assisters, so kills, deaths and assists come straight from
// the game instead of from the screen. The port is closed outside a match.
//
// Shape of the parts used (seen in real responses):
//   "activePlayer": { "riotId": "Name#TAG", "riotIdGameName": "Name", "summonerName": ... }
//                   (an { "error": ... } object when spectating or replaying)
//   "gameData": { "gameMode": "CLASSIC", "mapNumber": 11, "gameTime": 123.4 }
//   "events": { "Events": [
//       { "EventID": 3, "EventName": "ChampionKill", "EventTime": 98.1,
//         "KillerName": "...", "VictimName": "...", "Assisters": ["..."] },
//       { "EventName": "GameEnd", "Result": "Win" }, ... ] }
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::gamestate::GameEvent;
use crate::session::EventKind;

const URL: &str = "https://127.0.0.1:2999/liveclientdata/allgamedata";
/// How often to ask while in a match; event times come from the game's own
/// clock, so this only delays the markers, it doesn't shift them.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// The API can drop a request now and then (e.g. while the game hitches);
/// only this many failures in a row mean the match is over.
const MAX_MISSES: u32 = 3;

#[derive(Deserialize)]
struct AllGameData {
    #[serde(rename = "activePlayer", default)]
    active_player: ActivePlayer,
    #[serde(rename = "gameData")]
    game_data: GameData,
    #[serde(default)]
    events: Events,
}

#[derive(Deserialize, Default)]
struct ActivePlayer {
    #[serde(rename = "riotId", default)]
    riot_id: String,
    #[serde(rename = "riotIdGameName", default)]
    riot_id_game_name: String,
    #[serde(rename = "summonerName", default)]
    summoner_name: String,
}

#[derive(Deserialize)]
struct GameData {
    #[serde(rename = "gameMode", default)]
    game_mode: String,
    #[serde(rename = "mapNumber", default)]
    map_number: i64,
    #[serde(rename = "gameTime", default)]
    game_time: f64,
}

#[derive(Deserialize, Default)]
struct Events {
    #[serde(rename = "Events", default)]
    events: Vec<Event>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "EventID", default)]
    id: i64,
    #[serde(rename = "EventName", default)]
    name: String,
    #[serde(rename = "EventTime", default)]
    time: f64,
    #[serde(rename = "KillerName", default)]
    killer: String,
    #[serde(rename = "VictimName", default)]
    victim: String,
    #[serde(rename = "Assisters", default)]
    assisters: Vec<String>,
}

pub struct LeagueWatcher {
    agent: ureq::Agent,
    in_game: bool,
    ended: bool,
    misses: u32,
    /// Highest EventID already handled.
    last_event: i64,
    next_poll: Instant,
}

impl LeagueWatcher {
    pub fn new() -> Self {
        let tls = ureq::tls::TlsConfig::builder().disable_verification(true).build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(Duration::from_secs(2)))
            .proxy(None)
            .build()
            .into();
        Self { agent, in_game: false, ended: false, misses: 0, last_event: -1, next_poll: Instant::now() }
    }

    /// Asks the game for news (at most once per `POLL_INTERVAL`).
    pub fn poll(&mut self) -> Vec<GameEvent> {
        let now = Instant::now();
        if now < self.next_poll {
            return Vec::new();
        }
        self.next_poll = now + POLL_INTERVAL;
        match self.fetch() {
            Some(data) => {
                self.misses = 0;
                self.handle(data)
            }
            None if self.in_game => {
                self.misses += 1;
                if self.misses < MAX_MISSES {
                    return Vec::new();
                }
                self.in_game = false;
                vec![GameEvent::LeftMatch]
            }
            None => Vec::new(),
        }
    }

    fn fetch(&self) -> Option<AllGameData> {
        let mut response = self.agent.get(URL).call().ok()?;
        response.body_mut().read_json().ok()
    }

    fn handle(&mut self, data: AllGameData) -> Vec<GameEvent> {
        let mut out = Vec::new();
        let events = data.events.events;
        if !self.in_game {
            // Whatever happened before we noticed the match is before the
            // recording starts too.
            self.in_game = true;
            self.ended = false;
            self.last_event = events.iter().map(|e| e.id).max().unwrap_or(-1);
            let spectating = data.active_player.riot_id.is_empty() && data.active_player.summoner_name.is_empty();
            let mode = if spectating { "觀戰 / 重播".to_string() } else { mode_display_name(&data.game_data.game_mode) };
            out.push(GameEvent::MatchStarted { map: map_display_name(data.game_data.map_number), mode });
            return out;
        }

        let me = Me::new(&data.active_player);
        let now = data.game_data.game_time;
        for e in events.iter().filter(|e| e.id > self.last_event) {
            let ago = (now - e.time).max(0.0);
            match e.name.as_str() {
                "ChampionKill" => {
                    let kind = if me.is(&e.killer) {
                        Some(EventKind::Kill)
                    } else if me.is(&e.victim) {
                        Some(EventKind::Death)
                    } else if e.assisters.iter().any(|a| me.is(a)) {
                        Some(EventKind::Assist)
                    } else {
                        None
                    };
                    out.extend(kind.map(|kind| GameEvent::Event { kind, ago }));
                }
                "GameEnd" if !self.ended => {
                    self.ended = true;
                    out.push(GameEvent::MatchEnded);
                }
                _ => {}
            }
        }
        self.last_event = self.last_event.max(events.iter().map(|e| e.id).max().unwrap_or(-1));
        out
    }
}

impl Default for LeagueWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// The names the game may use for the player in events: depending on the
/// patch, the full Riot ID ("Name#TAG"), just the game name, or the old
/// summoner name.
struct Me(Vec<String>);

impl Me {
    fn new(p: &ActivePlayer) -> Self {
        let names = [&p.riot_id, &p.riot_id_game_name, &p.summoner_name]
            .into_iter()
            .flat_map(|n| [n.as_str(), base_name(n)])
            .filter(|n| !n.is_empty())
            .map(str::to_lowercase)
            .collect();
        Self(names)
    }

    fn is(&self, name: &str) -> bool {
        !name.is_empty() && [name, base_name(name)].iter().any(|n| self.0.contains(&n.to_lowercase()))
    }
}

/// "Name#TAG" -> "Name".
fn base_name(name: &str) -> &str {
    name.split('#').next().unwrap_or(name)
}

fn map_display_name(map: i64) -> String {
    let name = match map {
        11 => "召喚峽谷",
        12 => "嚎哭深淵",
        21 => "極限闖關",
        30 => "競技場",
        _ => "未知地圖",
    };
    name.to_string()
}

fn mode_display_name(code: &str) -> String {
    let name = match code {
        "CLASSIC" => "一般 / 積分",
        "ARAM" => "隨機單中",
        "KIWI" => "隨機單中:大混戰",
        "URF" => "阿福快打",
        "ARURF" => "隨機阿福快打",
        "CHERRY" => "競技場",
        "NEXUSBLITZ" => "極限闖關",
        "ONEFORALL" => "克隆大作戰",
        "ULTBOOK" => "終極魔典",
        "STRAWBERRY" => "蟲群倖存者",
        "PRACTICETOOL" => "練習工具",
        c if c.starts_with("TUTORIAL") => "新手教學",
        "" => "未知模式",
        other => other,
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(json: &str) -> AllGameData {
        serde_json::from_str(json).unwrap()
    }

    const PLAYER: &str = r#""activePlayer": {"riotId": "我#TW2", "riotIdGameName": "我", "summonerName": "我#TW2"}"#;

    #[test]
    fn reports_start_kda_and_end() {
        let mut w = LeagueWatcher::new();
        let start = data(&format!(
            r#"{{{PLAYER}, "gameData": {{"gameMode": "CLASSIC", "mapNumber": 11, "gameTime": 5.0}},
                "events": {{"Events": [{{"EventID": 0, "EventName": "GameStart", "EventTime": 0.1}}]}}}}"#
        ));
        assert_eq!(
            w.handle(start),
            [GameEvent::MatchStarted { map: "召喚峽谷".into(), mode: "一般 / 積分".into() }]
        );

        let later = data(&format!(
            r#"{{{PLAYER}, "gameData": {{"gameMode": "CLASSIC", "mapNumber": 11, "gameTime": 100.0}},
                "events": {{"Events": [
                    {{"EventID": 0, "EventName": "GameStart", "EventTime": 0.1}},
                    {{"EventID": 1, "EventName": "ChampionKill", "EventTime": 90.0, "KillerName": "我", "VictimName": "Foe", "Assisters": []}},
                    {{"EventID": 2, "EventName": "ChampionKill", "EventTime": 95.0, "KillerName": "Foe", "VictimName": "我#TW2", "Assisters": []}},
                    {{"EventID": 3, "EventName": "ChampionKill", "EventTime": 99.0, "KillerName": "Ally", "VictimName": "Foe", "Assisters": ["我"]}},
                    {{"EventID": 4, "EventName": "ChampionKill", "EventTime": 99.5, "KillerName": "Ally", "VictimName": "Foe", "Assisters": []}},
                    {{"EventID": 5, "EventName": "GameEnd", "EventTime": 100.0, "Result": "Win"}}
                ]}}}}"#
        ));
        assert_eq!(
            w.handle(later),
            [
                GameEvent::Event { kind: EventKind::Kill, ago: 10.0 },
                GameEvent::Event { kind: EventKind::Death, ago: 5.0 },
                GameEvent::Event { kind: EventKind::Assist, ago: 1.0 },
                GameEvent::MatchEnded,
            ]
        );
    }

    #[test]
    fn spectating_has_no_player() {
        let mut w = LeagueWatcher::new();
        let d = data(
            r#"{"activePlayer": {"error": "Spectator mode doesn't currently support this feature"},
                "gameData": {"gameMode": "CLASSIC", "mapNumber": 11, "gameTime": 5.0}, "events": {"Events": []}}"#,
        );
        match &w.handle(d)[0] {
            GameEvent::MatchStarted { mode, .. } => assert!(crate::gamestate::is_practice(mode)),
            e => panic!("unexpected {e:?}"),
        }
    }
}
