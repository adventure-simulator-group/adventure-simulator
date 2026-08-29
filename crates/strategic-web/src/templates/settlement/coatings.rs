//! Item coating and filth status presentation.

use adventuresim_stdb_client::{FilthOrigin, FilthSubstance};
use maud::{Markup, html};

pub(super) fn filth_status_bar(
    deposits: &[crate::spacetimedb::CharacterFilth],
    wetness_bps: u16,
) -> Markup {
    let dirt: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Dirt)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let blood: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let total = dirt
        .saturating_add(blood)
        .min(adventuresim_core::filth::MAX_FILTH);
    let dirt_width = f32::from(dirt.min(total));
    let blood_width = f32::from(blood.min(total.saturating_sub(dirt.min(total))));
    let (own_blood, foreign_blood, unknown_blood) = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .fold((0_u16, 0_u16, 0_u16), |mut amounts, deposit| {
            match deposit.origin {
                FilthOrigin::Own => amounts.0 = amounts.0.saturating_add(deposit.amount),
                FilthOrigin::Foreign => amounts.1 = amounts.1.saturating_add(deposit.amount),
                FilthOrigin::Unknown => amounts.2 = amounts.2.saturating_add(deposit.amount),
            }
            amounts
        });
    let summary = format!(
        "Current: {total}/100 — {dirt} dirt, {blood} blood ({own_blood} own, {foreign_blood} foreign, {unknown_blood} unknown)."
    );
    let details = format!(
        "Filth accumulates from travel, combat, and medical treatment. Dirt and blood fill the inner bar. Foreign blood can transmit bloodborne disease, with greater risk through open cuts and lesser risk through bandaged cuts. Soap is used automatically before rest to wash filth away.\n\nWetness is the blue outer bar behind filth. Rain and immersion add water; warmth and wind dry it. Wetness increases cold exposure. Current wetness: {}%.\n\n{summary}",
        wetness_bps / 100
    );
    html! {
        div class="coating-status" role="group" aria-label="Coatings" {
          strong class="metric-label filth-status-label" { "Filth" }
          div class="coating-track-stack" {
            div class="wetness-status" role="meter"
                aria-valuemin="0" aria-valuemax="100"
                aria-valuenow=(wetness_bps / 100)
                aria-label=(format!("Wetness {} out of 100", wetness_bps / 100)) {
              span class="wetness-track" aria-hidden="true" {
                  span style=(format!("width:{}%", wetness_bps / 100)) {}
              }
            }
            div class="filth-status" tabindex="0" role="meter" aria-valuemin="0" aria-valuemax="100"
                aria-valuenow=(total) aria-label=(format!("Filth {total} out of 100"))
                data-strategic-tooltip=(&details) {
              span class="filth-track" aria-hidden="true" {
                  @if dirt > 0 {
                      span class="filth-segment filth-dirt" style=(format!("width:{dirt_width}%"))
                          data-strategic-tooltip=(format!("Dirt\n{dirt}")) {}
                  }
                  @if blood > 0 {
                      span class="filth-segment filth-blood" style=(format!("width:{blood_width}%"))
                          data-strategic-tooltip=(format!("Blood\n{blood}")) {}
                  }
              }
            }
          }
        }
    }
}
