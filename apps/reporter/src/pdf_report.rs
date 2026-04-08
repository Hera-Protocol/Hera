use std::io::BufWriter;

use printpdf::{BuiltinFont, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use sha2::{Digest, Sha256};

use hera_types::{CanonicalEvent, Case};

use crate::{error::ReporterError, json_manifest::SignedManifest};

const PAGE_W: Mm = Mm(210.0);
const PAGE_H: Mm = Mm(297.0);

/// Builds the human-readable PDF rendering of the signed manifest. The PDF
/// includes the JSON manifest hash on the cover page. This allows a verifier to
/// confirm the PDF matches the signed JSON.
pub fn build_pdf(manifest: &SignedManifest, case: &Case) -> Result<Vec<u8>, ReporterError> {
    let manifest_bytes = manifest.pretty_json_bytes()?;
    let manifest_hash = hex_sha256(&manifest_bytes);

    let (doc, first_page, first_layer) =
        PdfDocument::new("Hera Compliance Report", PAGE_W, PAGE_H, "Cover");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|err| ReporterError::PdfGeneration(err.to_string()))?;
    let bold_font = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|err| ReporterError::PdfGeneration(err.to_string()))?;

    let first_page_layer = doc.get_page(first_page).get_layer(first_layer);
    let page_ctx = PdfPageContext {
        font: &font,
        bold_font: &bold_font,
    };

    add_cover_page(&page_ctx, &first_page_layer, case, manifest, &manifest_hash);
    add_summary_page(&doc, &font, &bold_font, manifest)?;
    add_timeline_page(&doc, &font, &bold_font, manifest)?;
    add_signature_page(&doc, &font, &bold_font, manifest, &manifest_hash);

    let mut writer = BufWriter::new(Vec::new());
    doc.save(&mut writer)
        .map_err(|err| ReporterError::PdfGeneration(err.to_string()))?;
    writer
        .into_inner()
        .map_err(|err| ReporterError::PdfGeneration(err.to_string()))
}

struct PdfPageContext<'a> {
    font: &'a printpdf::IndirectFontRef,
    bold_font: &'a printpdf::IndirectFontRef,
}

fn add_cover_page(
    ctx: &PdfPageContext<'_>,
    layer: &PdfLayerReference,
    case: &Case,
    manifest: &SignedManifest,
    manifest_hash: &str,
) {
    write_line(
        layer,
        ctx.bold_font,
        24.0,
        20.0,
        270.0,
        "Hera Compliance Report",
    );
    write_line(
        layer,
        ctx.font,
        12.0,
        20.0,
        252.0,
        &format!("Case ID: {}", case.id),
    );
    write_line(
        layer,
        ctx.font,
        12.0,
        20.0,
        244.0,
        &format!("Chain: {:?} / {:?}", case.chain, case.network),
    );
    write_line(
        layer,
        ctx.font,
        12.0,
        20.0,
        236.0,
        &format!("Generated At: {}", manifest.generated_at.to_rfc3339()),
    );
    write_line(
        layer,
        ctx.font,
        12.0,
        20.0,
        228.0,
        &format!("Event Count: {}", manifest.event_count),
    );
    write_line(
        layer,
        ctx.bold_font,
        12.0,
        20.0,
        212.0,
        "JSON Manifest SHA256",
    );
    write_line(layer, ctx.font, 10.0, 20.0, 204.0, manifest_hash);
}

fn add_summary_page(
    doc: &PdfDocumentReference,
    font: &printpdf::IndirectFontRef,
    bold_font: &printpdf::IndirectFontRef,
    manifest: &SignedManifest,
) -> Result<(), ReporterError> {
    let (page, layer) = doc.add_page(PAGE_W, PAGE_H, "Summary");
    let layer = doc.get_page(page).get_layer(layer);

    write_line(&layer, bold_font, 20.0, 20.0, 270.0, "Summary");

    let event_counts = summarize_event_types(&manifest.events);
    let asset_totals = summarize_asset_totals(&manifest.events)?;

    let mut y = 252.0;
    write_line(&layer, bold_font, 12.0, 20.0, y, "Event Counts");
    y -= 8.0;
    for (event_type, count) in event_counts {
        write_line(
            &layer,
            font,
            11.0,
            24.0,
            y,
            &format!("{event_type}: {count}"),
        );
        y -= 7.0;
    }

    y -= 6.0;
    write_line(&layer, bold_font, 12.0, 20.0, y, "Totals By Asset");
    y -= 8.0;
    for (asset, total) in asset_totals {
        // We format, we don't recompute: totals are derived by exact string
        // arithmetic over canonical event amounts, not from raw chain units.
        write_line(&layer, font, 11.0, 24.0, y, &format!("{asset}: {total}"));
        y -= 7.0;
    }

    Ok(())
}

fn add_timeline_page(
    doc: &PdfDocumentReference,
    font: &printpdf::IndirectFontRef,
    bold_font: &printpdf::IndirectFontRef,
    manifest: &SignedManifest,
) -> Result<(), ReporterError> {
    let (page, layer) = doc.add_page(PAGE_W, PAGE_H, "Timeline");
    let layer = doc.get_page(page).get_layer(layer);
    write_line(&layer, bold_font, 20.0, 20.0, 270.0, "Event Timeline");

    let mut y = 254.0;
    for event in &manifest.events {
        if y < 24.0 {
            break;
        }
        write_line(
            &layer,
            font,
            9.0,
            20.0,
            y,
            &format!(
                "{} | {:?} | {} {} | tx {}",
                event.timestamp.to_rfc3339(),
                event.event_type,
                event.amount,
                event.asset.symbol,
                event.txid
            ),
        );
        y -= 6.0;
    }

    Ok(())
}

fn add_signature_page(
    doc: &PdfDocumentReference,
    font: &printpdf::IndirectFontRef,
    bold_font: &printpdf::IndirectFontRef,
    manifest: &SignedManifest,
    manifest_hash: &str,
) {
    let (page, layer) = doc.add_page(PAGE_W, PAGE_H, "Signature");
    let layer = doc.get_page(page).get_layer(layer);
    write_line(&layer, bold_font, 20.0, 20.0, 270.0, "Signature");
    write_line(
        &layer,
        font,
        11.0,
        20.0,
        252.0,
        &format!("Algorithm: {}", manifest.signature.algorithm),
    );
    write_line(
        &layer,
        font,
        11.0,
        20.0,
        244.0,
        &format!("Signed At: {}", manifest.signature.signed_at.to_rfc3339()),
    );
    write_line(&layer, bold_font, 11.0, 20.0, 228.0, "Public Key");
    write_line(
        &layer,
        font,
        9.0,
        20.0,
        220.0,
        &manifest.signature.public_key_hex,
    );
    write_line(&layer, bold_font, 11.0, 20.0, 204.0, "Signature Hex");
    write_line(
        &layer,
        font,
        9.0,
        20.0,
        196.0,
        &manifest.signature.signature_hex,
    );
    write_line(&layer, bold_font, 11.0, 20.0, 180.0, "Manifest SHA256");
    write_line(&layer, font, 9.0, 20.0, 172.0, manifest_hash);
}

fn write_line(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    size: f64,
    x: f64,
    y: f64,
    text: &str,
) {
    layer.use_text(text, size as f32, Mm(x as f32), Mm(y as f32), font);
}

fn summarize_event_types(events: &[CanonicalEvent]) -> Vec<(String, usize)> {
    let mut counts = std::collections::BTreeMap::new();
    for event in events {
        let key = format!("{:?}", event.event_type);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

fn summarize_asset_totals(
    events: &[CanonicalEvent],
) -> Result<Vec<(String, String)>, ReporterError> {
    let mut totals: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for event in events {
        let key = event.asset.symbol.clone();
        let next = match totals.get(&key) {
            Some(current) => add_decimal_strings(current, &event.amount)?,
            None => event.amount.clone(),
        };
        totals.insert(key, next);
    }
    Ok(totals.into_iter().collect())
}

fn add_decimal_strings(left: &str, right: &str) -> Result<String, ReporterError> {
    let (left_int, left_scale) = parse_decimal_string(left)?;
    let (right_int, right_scale) = parse_decimal_string(right)?;
    let scale = left_scale.max(right_scale);
    let left_scaled = left_int
        .checked_mul(10u128.pow((scale - left_scale) as u32))
        .ok_or_else(|| ReporterError::PdfGeneration("decimal addition overflow".into()))?;
    let right_scaled = right_int
        .checked_mul(10u128.pow((scale - right_scale) as u32))
        .ok_or_else(|| ReporterError::PdfGeneration("decimal addition overflow".into()))?;
    let total = left_scaled
        .checked_add(right_scaled)
        .ok_or_else(|| ReporterError::PdfGeneration("decimal addition overflow".into()))?;
    Ok(format_scaled_decimal(total, scale))
}

fn parse_decimal_string(value: &str) -> Result<(u128, u8), ReporterError> {
    let mut parts = value.split('.');
    let integer = parts.next().unwrap_or_default();
    let fractional = parts.next();
    if parts.next().is_some() {
        return Err(ReporterError::PdfGeneration(
            "invalid decimal string in canonical event".into(),
        ));
    }

    let scale = fractional.map(|digits| digits.len() as u8).unwrap_or(0);
    let joined = match fractional {
        Some(digits) => format!("{integer}{digits}"),
        None => integer.to_string(),
    };

    let normalized = if joined.is_empty() { "0" } else { &joined };
    let parsed = normalized.parse::<u128>().map_err(|_| {
        ReporterError::PdfGeneration("invalid decimal string in canonical event".into())
    })?;
    Ok((parsed, scale))
}

fn format_scaled_decimal(value: u128, scale: u8) -> String {
    if scale == 0 {
        return value.to_string();
    }
    let power = 10u128.pow(scale as u32);
    let integer = value / power;
    let fractional = value % power;
    format!("{integer}.{fractional:0width$}", width = usize::from(scale))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}
