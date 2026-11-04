//! Share a member as a contact card (vCard 3.0): name, phone, address and birthday only.
//! ID numbers, notes and photos are private and never included.

use std::path::PathBuf;

use crate::models::Member;

/// Escape a vCard text value: `\`, `,`, `;` and newlines
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ',' => out.push_str("\\,"),
            ';' => out.push_str("\\;"),
            '\r' => {}
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// vCard 3.0 text (CRLF line endings) for a member
pub fn member_vcard(m: &Member) -> String {
    let name = escape(m.full_name.trim());
    let mut lines = vec![
        "BEGIN:VCARD".to_string(),
        "VERSION:3.0".to_string(),
        format!("N:{name};;;;"),
        format!("FN:{name}"),
    ];
    if let Some(phone) = m.phone.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        lines.push(format!("TEL;TYPE=CELL:{}", escape(phone)));
    }
    if let Some(address) = m.address.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        lines.push(format!("ADR;TYPE=HOME:;;{};;;;", escape(address)));
    }
    if let Some(ts) = m.birth_date {
        use chrono::{Local, TimeZone};
        if let Some(dt) = Local.timestamp_opt(ts, 0).single() {
            lines.push(format!("BDAY:{}", dt.format(crate::views::ISO_DATE_FMT)));
        }
    }
    lines.push("END:VCARD".to_string());
    let mut out = lines.join("\r\n");
    out.push_str("\r\n");
    out
}

/// File name stem: ASCII letters, digits, space, `-` and `_` only ("contact" when nothing is left)
fn safe_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() { "contact".to_string() } else { cleaned.to_string() }
}

/// Write the member's card to `<app files dir>/share/<safe name>.vcf`
pub fn write_member_vcard(m: &Member) -> std::io::Result<PathBuf> {
    let dir = crate::views::native::app_files_dir()?.join("share");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.vcf", safe_file_stem(&m.full_name)));
    std::fs::write(&path, member_vcard(m))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::member::MemberRole;

    fn member() -> Member {
        Member {
            id: "m1".into(),
            family_id: None,
            full_name: "Nguyễn Văn A".into(),
            birth_date: crate::views::date_str_to_ts("1990-05-17"),
            gender: None,
            phone: Some("0912 345 678".into()),
            id_number: Some("012345678901".into()),
            id_issue_date: None,
            id_issue_place: Some("Hà Nội".into()),
            address: Some("12 Lê Lợi, Q1; HCM\nVN \\ x".into()),
            role: MemberRole::Member,
            avatar_emoji: None,
            note: Some("secret note".into()),
            created_at: 0,
        }
    }

    #[test]
    fn vcard_has_contact_fields_with_crlf_and_escaping() {
        let v = member_vcard(&member());
        assert_eq!(
            v,
            "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Nguyễn Văn A;;;;\r\nFN:Nguyễn Văn A\r\n\
             TEL;TYPE=CELL:0912 345 678\r\nADR;TYPE=HOME:;;12 Lê Lợi\\, Q1\\; HCM\\nVN \\\\ x;;;;\r\n\
             BDAY:1990-05-17\r\nEND:VCARD\r\n"
        );
    }

    #[test]
    fn vcard_omits_private_and_missing_fields() {
        let mut m = member();
        m.phone = None;
        m.address = None;
        m.birth_date = None;
        let v = member_vcard(&m);
        assert!(!v.contains("TEL") && !v.contains("ADR") && !v.contains("BDAY"));
        assert!(!v.contains("012345678901") && !v.contains("secret") && !v.contains("Hà Nội"));
    }

    #[test]
    fn safe_file_stem_is_ascii_only() {
        assert_eq!(safe_file_stem("Nguyễn Văn A"), "Nguyn Vn A");
        assert_eq!(safe_file_stem("../x/y"), "xy");
        assert_eq!(safe_file_stem("ễ"), "contact");
    }
}
