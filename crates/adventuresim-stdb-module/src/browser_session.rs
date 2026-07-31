//! Private browser-session ownership authority.
//!
//! A browser cookie never contains character identifiers. The trusted
//! strategic gateway derives a pseudonymous owner key from the signed opaque
//! cookie and uses these rows to resolve the browser's roster and selection.

use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{character, starting_character_claim},
    strategic::{require_strategic_gateway, strategic_gateway_authority__view},
};

#[derive(Clone, Debug)]
#[table(accessor = browser_character_grant)]
pub struct BrowserCharacterGrant {
    /// A character can belong to only one browser owner.
    #[primary_key]
    pub character_id: u64,
    /// Numeric scan key for the trusted gateway projection. SpacetimeDB
    /// views cannot range-scan string indexes.
    #[index(btree)]
    pub character_scan_id: u64,
    #[index(btree)]
    pub owner_key: String,
    pub starting_request_key: String,
    pub granted_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = browser_character_selection)]
pub struct BrowserCharacterSelection {
    #[primary_key]
    pub owner_key: String,
    #[unique]
    pub character_id: u64,
    pub selected_micros: i64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendBrowserCharacterAccess {
    pub owner_key: String,
    pub character_id: u64,
    pub selected: bool,
}

fn valid_owner_key(owner_key: &str) -> bool {
    owner_key.len() == 64
        && owner_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn grant_browser_character_internal(
    ctx: &ReducerContext,
    owner_key: &str,
    character_id: u64,
    starting_request_key: &str,
) -> Result<(), String> {
    if !valid_owner_key(owner_key) {
        return Err("Browser owner key is malformed".into());
    }
    let claim = ctx
        .db
        .starting_character_claim()
        .request_key()
        .find(starting_request_key.to_owned())
        .ok_or("Starting-character claim not found")?;
    if claim.character_id != character_id || claim.owner_key != owner_key {
        return Err("Starting-character claim belongs to a different browser owner".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.temporary {
        return Err("Temporary characters cannot be granted to browser sessions".into());
    }
    if let Some(existing) = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(character_id)
    {
        return if existing.owner_key == owner_key
            && existing.starting_request_key == starting_request_key
        {
            Ok(())
        } else {
            Err("Character is already owned by a different browser session".into())
        };
    }
    ctx.db
        .browser_character_grant()
        .insert(BrowserCharacterGrant {
            character_id,
            character_scan_id: character_id,
            owner_key: owner_key.to_owned(),
            starting_request_key: starting_request_key.to_owned(),
            granted_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    Ok(())
}

#[reducer]
pub fn grant_browser_character(
    ctx: &ReducerContext,
    owner_key: String,
    character_id: u64,
    starting_request_key: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    grant_browser_character_internal(ctx, &owner_key, character_id, &starting_request_key)
}

#[reducer]
pub fn select_browser_character(
    ctx: &ReducerContext,
    owner_key: String,
    character_id: u64,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if !valid_owner_key(&owner_key) {
        return Err("Browser owner key is malformed".into());
    }
    let grant = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(character_id)
        .ok_or("Character is not granted to this browser session")?;
    if grant.owner_key != owner_key {
        return Err("Character is not granted to this browser session".into());
    }
    let selection = BrowserCharacterSelection {
        owner_key: owner_key.clone(),
        character_id,
        selected_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    };
    if ctx
        .db
        .browser_character_selection()
        .owner_key()
        .find(&owner_key)
        .is_some()
    {
        ctx.db
            .browser_character_selection()
            .owner_key()
            .update(selection);
    } else {
        ctx.db.browser_character_selection().insert(selection);
    }
    Ok(())
}

#[reducer]
pub fn clear_browser_character_selection(
    ctx: &ReducerContext,
    owner_key: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if !valid_owner_key(&owner_key) {
        return Err("Browser owner key is malformed".into());
    }
    ctx.db
        .browser_character_selection()
        .owner_key()
        .delete(&owner_key);
    Ok(())
}

fn is_strategic_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

#[view(accessor = backend_browser_character_access, public)]
pub fn backend_browser_character_access(ctx: &ViewContext) -> Vec<BackendBrowserCharacterAccess> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .browser_character_grant()
        .character_scan_id()
        .filter(0u64..)
        .map(|grant| {
            let selected = ctx
                .db
                .browser_character_selection()
                .owner_key()
                .find(&grant.owner_key)
                .is_some_and(|selection| selection.character_id == grant.character_id);
            BackendBrowserCharacterAccess {
                owner_key: grant.owner_key,
                character_id: grant.character_id,
                selected,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn ownership_authority_stays_private_and_gateway_scoped() {
        let source = include_str!("browser_session.rs");
        assert!(!source.contains("#[table(accessor = browser_character_grant, public)]"));
        assert!(!source.contains("#[table(accessor = browser_character_selection, public)]"));
        assert!(source.contains("require_strategic_gateway(ctx)?"));
        assert!(source.contains("if !is_strategic_gateway(ctx)"));
        assert!(source.contains("claim.owner_key != owner_key"));
        assert!(source.contains("grant.owner_key != owner_key"));
    }
}
