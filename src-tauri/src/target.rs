use crate::{
    error::{Error, Result},
    model::Target,
};
use url::Url;

pub fn valid_job(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
pub fn valid_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_=.".contains(&b))
}

/// Accept only Roblox HTTPS links or structured numeric/username targets.
/// Never pass an arbitrary URL or shell expression through to a process.
pub fn parse(input: &str) -> Result<Target> {
    let input = input.trim();
    if input.is_empty() || input.len() > 4096 {
        return Err(Error::Invalid("Enter a game link or place ID."));
    }
    if let Ok(place_id) = input.parse::<u64>() {
        if place_id == 0 {
            return Err(Error::Invalid("Place IDs must be positive."));
        }
        return Ok(Target::Place { place_id });
    }
    if let Some(name) = input.strip_prefix('@') {
        if (3..=20).contains(&name.len())
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            // User IDs are resolved by the Roblox API, not by the parser.
            return Ok(Target::Follow { user_id: 0 });
        }
        return Err(Error::Invalid("Invalid Roblox username."));
    }
    let url = Url::parse(input)
        .map_err(|_| Error::Invalid("Use a Roblox game link, place ID or @username."))?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("www.roblox.com" | "roblox.com"))
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::Invalid(
            "Only https://www.roblox.com links are accepted.",
        ));
    }
    let segments: Vec<_> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    if segments.first() == Some(&"users") {
        let user_id = segments
            .get(1)
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or(Error::Invalid("Invalid profile link."))?;
        if user_id == 0 {
            return Err(Error::Invalid("Invalid profile link."));
        }
        return Ok(Target::Follow { user_id });
    }
    if segments.first() != Some(&"games") {
        return Err(Error::Invalid("This is not a game or profile link."));
    }
    let place_id = segments
        .get(1)
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or(Error::Invalid("The game link has no valid place ID."))?;
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    if let Some(code) = query.get("accessCode") {
        if !valid_code(code) {
            return Err(Error::Invalid("Invalid reserved-server access code."));
        }
        return Ok(Target::Reserved {
            place_id,
            access_code: code.clone().into_owned(),
        });
    }
    if let Some(code) = query
        .get("privateServerLinkCode")
        .or_else(|| query.get("linkCode"))
    {
        if !valid_code(code) {
            return Err(Error::Invalid("Invalid private server code."));
        }
        return Ok(Target::Private {
            place_id,
            link_code: code.clone().into_owned(),
        });
    }
    if let Some(job_id) = query.get("game-id").or_else(|| query.get("jobId")) {
        if !valid_job(job_id) {
            return Err(Error::Invalid("Invalid server job ID."));
        }
        return Ok(Target::Job {
            place_id,
            job_id: job_id.to_ascii_lowercase(),
        });
    }
    if query.contains_key("code") || segments.get(2).is_some_and(|s| *s == "share") {
        return Err(Error::Invalid("Open this share link in Roblox and copy the resolved game/private-server link. Share-code resolution is not enabled in this release."));
    }
    Ok(Target::Place { place_id })
}

pub fn launcher_url(target: &Target, tracker: u64) -> Result<String> {
    let base = "https://www.roblox.com/Game/PlaceLauncher.ashx";
    let mut url = Url::parse(base).map_err(|_| Error::Internal)?;
    let attempt = uuid::Uuid::new_v4().to_string();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("browserTrackerId", &tracker.to_string());
        match target {
            Target::Place { place_id } => {
                query
                    .append_pair("request", "RequestGame")
                    .append_pair("placeId", &place_id.to_string());
            }
            Target::Job { place_id, job_id } => {
                if !valid_job(job_id) {
                    return Err(Error::Invalid("Invalid job ID."));
                }
                query
                    .append_pair("request", "RequestGameJob")
                    .append_pair("placeId", &place_id.to_string())
                    .append_pair("gameId", job_id);
            }
            Target::Private {
                place_id,
                link_code,
            } => {
                if !valid_code(link_code) {
                    return Err(Error::Invalid("Invalid private server code."));
                }
                query
                    .append_pair("request", "RequestPrivateGame")
                    .append_pair("placeId", &place_id.to_string())
                    .append_pair("linkCode", link_code);
            }
            Target::Reserved {
                place_id,
                access_code,
            } => {
                if !valid_code(access_code) {
                    return Err(Error::Invalid("Invalid reserved-server access code."));
                }
                query
                    .append_pair("request", "RequestPrivateGame")
                    .append_pair("placeId", &place_id.to_string())
                    .append_pair("accessCode", access_code);
            }
            Target::Follow { user_id } => {
                if *user_id == 0 {
                    return Err(Error::Invalid("Unresolved user."));
                }
                query
                    .append_pair("request", "RequestFollowUser")
                    .append_pair("userId", &user_id.to_string());
            }
        }
        query
            .append_pair("isPlayTogetherGame", "false")
            .append_pair("joinAttemptId", &attempt)
            .append_pair("joinAttemptOrigin", "PlayButton");
    }
    Ok(url.into())
}
pub fn launch_uri(ticket: &str, target: &Target, tracker: u64, timestamp: u128) -> Result<String> {
    use url::form_urlencoded::byte_serialize;
    if ticket.is_empty() || ticket.len() > 16_384 || ticket.chars().any(char::is_control) {
        return Err(Error::Internal);
    }
    let ticket: String = byte_serialize(ticket.as_bytes()).collect();
    let launcher: String = byte_serialize(launcher_url(target, tracker)?.as_bytes()).collect();
    Ok(format!("roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{timestamp}+placelauncherurl:{launcher}+browsertrackerid:{tracker}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_places_and_jobs() {
        assert_eq!(parse("1818").unwrap(), Target::Place { place_id: 1818 });
        assert_eq!(parse("https://www.roblox.com/games/1818/Classic?game-id=AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE").unwrap(),
            Target::Job { place_id: 1818, job_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into() });
    }
    #[test]
    fn private_server_keeps_opaque_code() {
        assert_eq!(
            parse("https://www.roblox.com/games/1818?privateServerLinkCode=abc_123-DEF").unwrap(),
            Target::Private {
                place_id: 1818,
                link_code: "abc_123-DEF".into()
            }
        );
    }
    #[test]
    fn reserved_access_code_is_not_an_invitation_link_code() {
        let target = parse("https://www.roblox.com/games/123?accessCode=ABC_123").unwrap();
        assert!(
            matches!(target, Target::Reserved { ref access_code, .. } if access_code.as_str() == "ABC_123")
        );
        let uri = launch_uri("ticket+/=", &target, 123, 456).unwrap();
        assert!(uri.contains("accessCode%3DABC_123"));
        assert!(!uri.contains("linkCode"));
    }
    #[test]
    fn rejects_untrusted_links_and_commands() {
        for value in [
            "https://evil.example/games/1818",
            "https://www.roblox.com.evil.example/games/1818",
            "https://www.roblox.com@evil.example/games/1818",
            "http://www.roblox.com/games/1818",
            "0",
            "file:///c:/windows",
            "https://www.roblox.com/games/1&calc",
            "https://www.roblox.com/games/1?game-id=bad",
        ] {
            assert!(parse(value).is_err(), "{value}");
        }
    }
    #[test]
    fn uri_encodes_ticket_and_launcher() {
        let uri = launch_uri(
            "ticket+/&=secret",
            &Target::Place { place_id: 1818 },
            1234,
            100,
        )
        .unwrap();
        assert!(uri.contains("gameinfo:ticket%2B%2F%26%3Dsecret+"));
        assert!(uri.contains("placelauncherurl:https%3A%2F%2F"));
        assert!(!uri.contains("placeId=1818"));
    }
}
