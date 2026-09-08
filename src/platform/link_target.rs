use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};
type Location = (Option<u32>, Option<u32>);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinkTarget {
    Web(String),
    File {
        path: PathBuf,
        line: Option<u32>,
        column: Option<u32>,
    },
}
pub(crate) fn parse_target(raw: &str) -> Result<LinkTarget> {
    valid_text(raw)?;
    if is_http_scheme(raw) {
        return parse_web(raw);
    }
    if starts_with_scheme(raw, "file") {
        return parse_file_url(raw);
    }
    let (path, fragment) = split_fragment(raw)?;
    reject_scheme(path)?;
    parse_local(path, fragment.as_deref(), false)
}
fn parse_web(raw: &str) -> Result<LinkTarget> {
    let scheme_end = raw.find("://").context("web link is missing //")?;
    let rest = &raw[scheme_end + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        bail!("malformed web link");
    }
    if authority.starts_with('[') {
        let close = authority.find(']').context("malformed web host")?;
        if close == 1
            || (!authority[close + 1..].is_empty() && !authority[close + 1..].starts_with(':'))
        {
            bail!("malformed web host");
        }
        if let Some(port) = authority[close + 1..].strip_prefix(':') {
            validate_port(port)?;
        }
    } else {
        let mut pieces = authority.split(':');
        let host = pieces.next().unwrap_or_default();
        if host.is_empty() || pieces.clone().count() > 1 {
            bail!("malformed web host");
        }
        if let Some(port) = pieces.next() {
            validate_port(port)?;
        }
    }
    if raw.contains('\\') || raw.chars().any(char::is_whitespace) {
        bail!("web link contains whitespace or a backslash");
    }
    valid_percent_encoding(raw)?;
    Ok(LinkTarget::Web(raw.to_owned()))
}
fn parse_file_url(raw: &str) -> Result<LinkTarget> {
    if raw.len() < 7 || !starts_with_scheme(raw, "file") || !raw[7..].starts_with('/') {
        bail!("file link must use a local file URI");
    }
    let (encoded_path, fragment) = split_fragment(&raw[7..])?;
    if encoded_path.contains('?') {
        bail!("file link queries are unsupported");
    }
    valid_percent_encoding(encoded_path)?;
    let decoded = percent_decode(encoded_path)?;
    #[cfg(windows)]
    let decoded = decoded
        .strip_prefix('/')
        .filter(|path| is_drive_path(path))
        .unwrap_or(&decoded)
        .to_owned();
    parse_local(&decoded, fragment.as_deref(), true)
}
fn parse_local(raw_path: &str, fragment: Option<&str>, file_url: bool) -> Result<LinkTarget> {
    valid_text(raw_path)?;
    let fragment_location = fragment.map(parse_fragment).transpose()?;
    let (path, suffix_location) = split_line_ref(raw_path)?;
    if fragment_location.is_some() && suffix_location.is_some() {
        bail!("file link has two line references");
    }
    let (line, column) = fragment_location
        .or(suffix_location)
        .unwrap_or((None, None));
    let path = validate_local_path(path, file_url)?;
    Ok(LinkTarget::File { path, line, column })
}
fn validate_local_path(raw: &str, file_url: bool) -> Result<PathBuf> {
    if raw.is_empty() || is_forbidden_namespace(raw) {
        bail!("local link path is empty or uses a network/device namespace");
    }
    #[cfg(windows)]
    let native_absolute = is_drive_path(raw);
    #[cfg(not(windows))]
    let native_absolute = raw.starts_with('/');
    if file_url && !native_absolute {
        bail!("local link path is not native to this platform");
    }
    if !file_url && native_absolute && !cfg!(windows) {
        bail!("plain local links must be repository-relative");
    }
    #[cfg(windows)]
    if !is_drive_path(raw) && raw.starts_with(['/', '\\']) {
        bail!("Windows local links must use a drive path");
    }
    if raw.starts_with("//") || raw.starts_with(r"\\") {
        bail!("network paths are not opened");
    }
    let path = Path::new(raw);
    for component in path.components() {
        match component {
            Component::ParentDir => bail!("local link escapes its repository"),
            Component::Normal(name)
                if name.to_str().is_some_and(crate::git::path::is_git_dir_name) =>
            {
                bail!("links into .git are not opened")
            }
            _ => {}
        }
    }
    #[cfg(windows)]
    if raw.contains(':') && (!is_drive_path(raw) || raw[2..].contains(':')) {
        bail!("alternate data streams are not opened");
    }
    Ok(PathBuf::from(raw))
}
fn parse_fragment(fragment: &str) -> Result<Location> {
    let rest = fragment
        .strip_prefix('L')
        .context("unsupported link fragment")?;
    let rest = if let Some((start, end)) = rest.split_once('-') {
        let end = end.strip_prefix('L').unwrap_or(end);
        if !is_line_location(end) {
            bail!("malformed line range");
        }
        start
    } else {
        rest
    };
    let (line, column) = rest
        .split_once(':')
        .or_else(|| rest.split_once('C'))
        .map_or((rest, None), |(line, column)| (line, Some(column)));
    let line = parse_positive(line, "line")?;
    let column = column
        .map(|column| parse_positive(column, "column"))
        .transpose()?;
    Ok((Some(line), column))
}
fn split_line_ref(path: &str) -> Result<(&str, Option<Location>)> {
    let path = path
        .rsplit_once('-')
        .filter(|(start, end)| {
            let Some((_, line_part)) = start.rsplit_once(':') else {
                return false;
            };
            line_part.bytes().all(|byte| byte.is_ascii_digit()) && is_line_location(end)
        })
        .map_or(path, |(start, _)| start);
    let Some((prefix, last)) = path.rsplit_once(':') else {
        return Ok((path, None));
    };
    if !last.bytes().all(|b| b.is_ascii_digit()) {
        return Ok((path, None));
    }
    let value = parse_positive(last, "line or column")?;
    let Some((base, line)) = prefix.rsplit_once(':') else {
        return Ok((prefix, Some((Some(value), None))));
    };
    if line.bytes().all(|b| b.is_ascii_digit()) && (!base.contains(':') || is_drive_path(base)) {
        return Ok((
            base,
            Some((Some(parse_positive(line, "line")?), Some(value))),
        ));
    }
    Ok((prefix, Some((Some(value), None))))
}
fn is_line_location(value: &str) -> bool {
    let (line, column) = value
        .split_once(':')
        .or_else(|| value.split_once('C'))
        .map_or((value, None), |(line, column)| (line, Some(column)));
    !line.is_empty()
        && line.bytes().all(|byte| byte.is_ascii_digit())
        && column.is_none_or(|column| {
            !column.is_empty() && column.bytes().all(|byte| byte.is_ascii_digit())
        })
}
fn parse_positive(value: &str, label: &str) -> Result<u32> {
    let number = value
        .parse::<u32>()
        .with_context(|| format!("invalid {label}"))?;
    if number == 0 {
        bail!("{label} must be greater than zero");
    }
    Ok(number)
}
fn reject_scheme(path: &str) -> Result<()> {
    let Some(colon) = path.find(':') else {
        return Ok(());
    };
    if is_drive_path(path) {
        return Ok(());
    }
    if path[..colon].bytes().all(|byte| byte.is_ascii_alphabetic()) {
        bail!("unsupported link scheme");
    }
    Ok(())
}
fn validate_port(port: &str) -> Result<()> {
    if port.is_empty() || port.parse::<u16>().is_err() {
        bail!("malformed web port");
    }
    Ok(())
}
fn valid_percent_encoding(text: &str) -> Result<()> {
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                bail!("malformed percent escape");
            }
            let value = u8::from_str_radix(&text[index + 1..index + 3], 16);
            if value.is_ok_and(|byte| byte.is_ascii_control()) {
                bail!("percent escape contains a control character");
            }
        }
    }
    Ok(())
}
fn percent_decode(text: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let raw = text.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' {
            bytes.push(u8::from_str_radix(&text[index + 1..index + 3], 16)?);
            index += 3;
        } else {
            let character = text[index..].chars().next().context("invalid UTF-8")?;
            let mut encoded = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            index += character.len_utf8();
        }
    }
    String::from_utf8(bytes).context("file URI is not valid UTF-8")
}
fn split_fragment(raw: &str) -> Result<(&str, Option<String>)> {
    let Some((path, fragment)) = raw.split_once('#') else {
        return Ok((raw, None));
    };
    if fragment.is_empty() {
        bail!("empty link fragment");
    }
    Ok((path, Some(fragment.to_owned())))
}
fn valid_text(text: &str) -> Result<()> {
    if text.is_empty() || text.chars().any(char::is_control) {
        bail!("link contains an empty or control character");
    }
    Ok(())
}
fn starts_with_scheme(text: &str, scheme: &str) -> bool {
    text.as_bytes()
        .get(..scheme.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme.as_bytes()))
        && text.as_bytes().get(scheme.len()) == Some(&b':')
}
fn is_http_scheme(text: &str) -> bool {
    text.as_bytes()
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"http://"))
        || text
            .as_bytes()
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"https://"))
}
fn is_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}
fn is_forbidden_namespace(path: &str) -> bool {
    path.starts_with(r"\\")
        || path.starts_with("//")
        || path.starts_with(r"\\?\")
        || path.starts_with(r"\\.\")
        || path.starts_with("//?/")
        || path.starts_with("//./")
}
#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
