use std::{
    collections::{BTreeMap, BTreeSet},
    io::BufWriter,
};

use chrono::{DateTime, Utc};
use printpdf::{BuiltinFont, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use sha2::{Digest, Sha256};

use hera_types::{CanonicalEvent, Case};

use crate::{error::ReporterError, json_manifest::SignedManifest};

const PAGE_W: Mm = Mm(210.0);
const PAGE_H: Mm = Mm(297.0);
const LEFT_MARGIN: f64 = 20.0;
const TOP_Y: f64 = 270.0;
const BOTTOM_Y: f64 = 20.0;
const WRAP_WIDTH: usize = 92;

pub fn build_pdf(manifest: &SignedManifest, case: &Case) -> Result<Vec<u8>, ReporterError> {
    let manifest_bytes = manifest.pretty_json_bytes()?;
    let manifest_hash = hex_sha256(&manifest_bytes);
    let report = ReportContext::from_manifest(manifest);

    let (doc, first_page, first_layer) = PdfDocument::new(
        "Hera Protocol Compliance Audit Report",
        PAGE_W,
        PAGE_H,
        "Cover",
    );
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|err| ReporterError::PdfGeneration(err.to_string()))?;
    let bold_font = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|err| ReporterError::PdfGeneration(err.to_string()))?;

    let cover_layer = doc.get_page(first_page).get_layer(first_layer);
    let ctx = PdfPageContext {
        font: &font,
        bold_font: &bold_font,
    };

    add_cover_page(&ctx, &cover_layer, case, manifest, &report);
    add_scope_page(&doc, &ctx, case, &report)?;
    add_summary_page(&doc, &ctx, manifest, &report)?;
    add_timeline_pages(&doc, &ctx, manifest)?;
    add_controls_page(&doc, &ctx)?;
    add_signature_page(&doc, &ctx, manifest, &manifest_hash)?;

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

struct ReportContext {
    audit_start: Option<DateTime<Utc>>,
    audit_end: Option<DateTime<Utc>>,
    distinct_assets: usize,
    distinct_networks: usize,
    memo_count: usize,
}

impl ReportContext {
    fn from_manifest(manifest: &SignedManifest) -> Self {
        let audit_start = manifest
            .events
            .iter()
            .map(|event| event.timestamp)
            .min();
        let audit_end = manifest
            .events
            .iter()
            .map(|event| event.timestamp)
            .max();
        let distinct_assets = manifest
            .events
            .iter()
            .map(|event| event.asset.symbol.clone())
            .collect::<BTreeSet<_>>()
            .len();
        let distinct_networks = manifest
            .events
            .iter()
            .map(|event| format!("{:?}", event.network))
            .collect::<BTreeSet<_>>()
            .len();
        let memo_count = manifest
            .events
            .iter()
            .filter(|event| event.memo.present)
            .count();

        Self {
            audit_start,
            audit_end,
            distinct_assets,
            distinct_networks,
            memo_count,
        }
    }
}

fn add_cover_page(
    ctx: &PdfPageContext<'_>,
    layer: &PdfLayerReference,
    case: &Case,
    manifest: &SignedManifest,
    report: &ReportContext,
) {
    let mut y = TOP_Y;

    write_line(
        layer,
        ctx.bold_font,
        24.0,
        LEFT_MARGIN,
        y,
        "Hera Protocol Compliance Audit Report",
    );
    y -= 10.0;
    write_line(
        layer,
        ctx.font,
        11.0,
        LEFT_MARGIN,
        y,
        "Confidential | Customer Demo Artifact",
    );
    y -= 18.0;

    write_key_value(layer, ctx, &mut y, "Report ID", &case.id.to_string());
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Generated",
        &manifest.generated_at.to_rfc3339(),
    );
    write_key_value(layer, ctx, &mut y, "Chain", &format!("{:?}", case.chain));
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Network",
        &format!("{:?}", case.network),
    );
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Audit Period",
        &format_audit_period(report.audit_start, report.audit_end),
    );
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Status",
        "Signed canonical JSON artifact available",
    );

    y -= 10.0;
    write_section_title(layer, ctx, &mut y, "Audit Scope");
    write_wrapped_block(
        layer,
        ctx.font,
        11.0,
        LEFT_MARGIN,
        &mut y,
        "This report summarizes user-authorized shielded activity reconstructed from read-only viewing capability. Hera does not require spending authority and does not move funds.",
    );
    write_wrapped_block(
        layer,
        ctx.font,
        11.0,
        LEFT_MARGIN,
        &mut y,
        "The PDF is derived from the signed canonical JSON manifest. Chain-specific interpretation remains separate below the normalization layer, while the exported report stays chain-agnostic above it.",
    );

    y -= 4.0;
    write_section_title(layer, ctx, &mut y, "At A Glance");
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Transactions",
        &manifest.event_count.to_string(),
    );
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Asset Types",
        &report.distinct_assets.to_string(),
    );
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Networks Observed",
        &report.distinct_networks.to_string(),
    );
    write_key_value(
        layer,
        ctx,
        &mut y,
        "Events With Memo Evidence",
        &report.memo_count.to_string(),
    );
}

fn add_scope_page(
    doc: &PdfDocumentReference,
    ctx: &PdfPageContext<'_>,
    case: &Case,
    report: &ReportContext,
) -> Result<(), ReporterError> {
    let (page, layer) = doc.add_page(PAGE_W, PAGE_H, "Scope");
    let layer = doc.get_page(page).get_layer(layer);
    let mut y = TOP_Y;

    write_page_title(&layer, ctx, &mut y, "1. Audit Scope");
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Networks Scanned",
        &format!("{:?} ({:?})", case.chain, case.network),
    );
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Viewing Scope",
        "Read-only viewing capability supplied for authorized compliance review",
    );
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Audit Period",
        &format_audit_period(report.audit_start, report.audit_end),
    );

    y -= 8.0;
    write_section_title(&layer, ctx, &mut y, "Included In This Report");
    for line in [
        "Owned shielded activity reconstructed from the supplied viewing scope",
        "Normalized event timeline covering receives, sends, shielding, unshielding, and fees",
        "Asset-level totals derived from canonical event amounts",
        "Evidence references and detached signature metadata for independent verification",
    ] {
        write_bullet(&layer, ctx, &mut y, line);
    }

    y -= 4.0;
    write_section_title(&layer, ctx, &mut y, "Explicit Limitations");
    for line in [
        "Counterparty visibility may remain unknown or partial because shielded protocols intentionally limit disclosure",
        "The PDF is a human-readable derivative; the signed JSON manifest remains the machine-verifiable source of truth",
        "Compliance conclusions still require institutional review and policy interpretation",
    ] {
        write_bullet(&layer, ctx, &mut y, line);
    }

    Ok(())
}

fn add_summary_page(
    doc: &PdfDocumentReference,
    ctx: &PdfPageContext<'_>,
    manifest: &SignedManifest,
    report: &ReportContext,
) -> Result<(), ReporterError> {
    let (page, layer) = doc.add_page(PAGE_W, PAGE_H, "Summary");
    let layer = doc.get_page(page).get_layer(layer);
    let mut y = TOP_Y;

    write_page_title(&layer, ctx, &mut y, "2. Audit Summary");
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Canonical Events",
        &manifest.event_count.to_string(),
    );
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Distinct Assets",
        &report.distinct_assets.to_string(),
    );
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Distinct Networks",
        &report.distinct_networks.to_string(),
    );
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Memo Evidence Count",
        &report.memo_count.to_string(),
    );

    let event_counts = summarize_event_types(&manifest.events);
    let asset_totals = summarize_asset_totals(&manifest.events)?;

    y -= 8.0;
    write_section_title(&layer, ctx, &mut y, "Event Counts");
    for (event_type, count) in event_counts {
        write_key_value(&layer, ctx, &mut y, &event_type, &count.to_string());
    }

    y -= 6.0;
    write_section_title(&layer, ctx, &mut y, "Totals By Asset");
    for (asset, total) in asset_totals {
        write_key_value(&layer, ctx, &mut y, &asset, &total);
    }

    Ok(())
}

fn add_timeline_pages(
    doc: &PdfDocumentReference,
    ctx: &PdfPageContext<'_>,
    manifest: &SignedManifest,
) -> Result<(), ReporterError> {
    for (page_index, chunk) in manifest.events.chunks(24).enumerate() {
        let (page, layer) = doc.add_page(
            PAGE_W,
            PAGE_H,
            if page_index == 0 {
                "Timeline".to_string()
            } else {
                format!("Timeline {}", page_index + 1)
            },
        );
        let layer = doc.get_page(page).get_layer(layer);
        let mut y = TOP_Y;

        write_page_title(
            &layer,
            ctx,
            &mut y,
            &format!("3. Transaction Ledger{}", page_suffix(page_index)),
        );

        for event in chunk {
            if y < BOTTOM_Y + 20.0 {
                break;
            }

            let summary = format!(
                "{} | {:?} | {} {} | block {}",
                event.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                event.event_type,
                event.amount,
                event.asset.symbol,
                event.block_height
            );
            write_wrapped_block(&layer, ctx.font, 10.0, LEFT_MARGIN, &mut y, &summary);
            write_wrapped_block(
                &layer,
                ctx.font,
                9.0,
                LEFT_MARGIN + 4.0,
                &mut y,
                &format!(
                    "tx {} | counterparty {} | evidence {}",
                    event.txid,
                    format_counterparty(event),
                    summarize_evidence_refs(event)
                ),
            );
            y -= 2.0;
        }
    }

    Ok(())
}

fn add_controls_page(
    doc: &PdfDocumentReference,
    ctx: &PdfPageContext<'_>,
) -> Result<(), ReporterError> {
    let (page, layer) = doc.add_page(PAGE_W, PAGE_H, "Controls");
    let layer = doc.get_page(page).get_layer(layer);
    let mut y = TOP_Y;

    write_page_title(&layer, ctx, &mut y, "4. Security And Compliance Notes");

    write_section_title(&layer, ctx, &mut y, "Viewing Key Handling");
    for line in [
        "Submission occurs over TLS and viewing capability is encrypted immediately on arrival",
        "The application flow is designed for read-only scanning and does not require spending authority",
        "Tenant isolation, audit logging, and signed export artifacts remain part of the core control surface",
    ] {
        write_bullet(&layer, ctx, &mut y, line);
    }

    y -= 4.0;
    write_section_title(&layer, ctx, &mut y, "Reporting Notes");
    for line in [
        "Canonical JSON is the source of truth for downstream verification and machine processing",
        "PDF output is generated from normalized events and detached report signature metadata",
        "Unknown or partial counterparty visibility is preserved explicitly and never filled in speculatively",
    ] {
        write_bullet(&layer, ctx, &mut y, line);
    }

    y -= 4.0;
    write_section_title(&layer, ctx, &mut y, "Institutional Review Reminder");
    write_wrapped_block(
        &layer,
        ctx.font,
        11.0,
        LEFT_MARGIN,
        &mut y,
        "This artifact is intended to support compliance review, internal controls, and external explanation of reconstructed shielded activity. Final legal determinations remain the responsibility of the institution and its counsel.",
    );

    Ok(())
}

fn add_signature_page(
    doc: &PdfDocumentReference,
    ctx: &PdfPageContext<'_>,
    manifest: &SignedManifest,
    manifest_hash: &str,
) -> Result<(), ReporterError> {
    let (page, layer) = doc.add_page(PAGE_W, PAGE_H, "Signature");
    let layer = doc.get_page(page).get_layer(layer);
    let mut y = TOP_Y;

    write_page_title(&layer, ctx, &mut y, "5. Report Signature");
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Signature Algorithm",
        &manifest.signature.algorithm,
    );
    write_key_value(
        &layer,
        ctx,
        &mut y,
        "Signed At",
        &manifest.signature.signed_at.to_rfc3339(),
    );
    write_key_value(&layer, ctx, &mut y, "Manifest SHA256", manifest_hash);

    y -= 8.0;
    write_section_title(&layer, ctx, &mut y, "Public Key");
    write_wrapped_block(
        &layer,
        ctx.font,
        9.0,
        LEFT_MARGIN,
        &mut y,
        &manifest.signature.public_key_hex,
    );

    y -= 4.0;
    write_section_title(&layer, ctx, &mut y, "Signature");
    write_wrapped_block(
        &layer,
        ctx.font,
        9.0,
        LEFT_MARGIN,
        &mut y,
        &manifest.signature.signature_hex,
    );

    y -= 4.0;
    write_wrapped_block(
        &layer,
        ctx.font,
        10.0,
        LEFT_MARGIN,
        &mut y,
        "Independent verification should be performed against the canonical JSON artifact. If a PDF rendering and canonical JSON ever diverge, the signed JSON controls.",
    );

    Ok(())
}

fn write_page_title(layer: &PdfLayerReference, ctx: &PdfPageContext<'_>, y: &mut f64, title: &str) {
    write_line(layer, ctx.bold_font, 20.0, LEFT_MARGIN, *y, title);
    *y -= 14.0;
}

fn write_section_title(
    layer: &PdfLayerReference,
    ctx: &PdfPageContext<'_>,
    y: &mut f64,
    title: &str,
) {
    write_line(layer, ctx.bold_font, 12.0, LEFT_MARGIN, *y, title);
    *y -= 8.0;
}

fn write_key_value(
    layer: &PdfLayerReference,
    ctx: &PdfPageContext<'_>,
    y: &mut f64,
    key: &str,
    value: &str,
) {
    write_line(
        layer,
        ctx.bold_font,
        11.0,
        LEFT_MARGIN,
        *y,
        &format!("{key}:"),
    );
    *y -= 6.0;
    write_wrapped_block(layer, ctx.font, 11.0, LEFT_MARGIN + 4.0, y, value);
    *y -= 2.0;
}

fn write_bullet(layer: &PdfLayerReference, ctx: &PdfPageContext<'_>, y: &mut f64, text: &str) {
    for (index, line) in wrap_text(text, WRAP_WIDTH - 4).into_iter().enumerate() {
        let prefix = if index == 0 { "- " } else { "  " };
        write_line(
            layer,
            ctx.font,
            11.0,
            LEFT_MARGIN,
            *y,
            &format!("{prefix}{line}"),
        );
        *y -= 6.0;
    }
    *y -= 1.0;
}

fn write_wrapped_block(
    layer: &PdfLayerReference,
    font: &printpdf::IndirectFontRef,
    size: f64,
    x: f64,
    y: &mut f64,
    text: &str,
) {
    for line in wrap_text(text, WRAP_WIDTH) {
        write_line(layer, font, size, x, *y, &line);
        *y -= 6.0;
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };

        if candidate_len > width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else if current.is_empty() {
            current = word.to_string();
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn summarize_event_types(events: &[CanonicalEvent]) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::new();
    for event in events {
        let key = format!("{:?}", event.event_type);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

fn summarize_asset_totals(
    events: &[CanonicalEvent],
) -> Result<Vec<(String, String)>, ReporterError> {
    let mut totals: BTreeMap<String, String> = BTreeMap::new();

    for event in events {
        let entry = totals
            .entry(event.asset.symbol.clone())
            .or_insert_with(|| "0".to_string())
            .clone();
        let total = add_decimal_strings(&entry, &event.amount)?;
        totals.insert(event.asset.symbol.clone(), total);
    }

    Ok(totals.into_iter().collect())
}

fn add_decimal_strings(left: &str, right: &str) -> Result<String, ReporterError> {
    let left_parts: Vec<_> = left.split('.').collect();
    let right_parts: Vec<_> = right.split('.').collect();
    if left_parts.len() > 2 || right_parts.len() > 2 {
        return Err(ReporterError::PdfGeneration(
            "invalid decimal amount in canonical event".to_string(),
        ));
    }

    let left_whole = left_parts[0];
    let right_whole = right_parts[0];
    let left_fraction = left_parts.get(1).copied().unwrap_or("");
    let right_fraction = right_parts.get(1).copied().unwrap_or("");
    let scale = left_fraction.len().max(right_fraction.len());

    let left_scaled = format!("{left_whole}{:0<scale$}", left_fraction, scale = scale);
    let right_scaled = format!("{right_whole}{:0<scale$}", right_fraction, scale = scale);

    let sum = left_scaled
        .parse::<i128>()
        .and_then(|left_value| {
            right_scaled
                .parse::<i128>()
                .map(|right_value| left_value + right_value)
        })
        .map_err(|_| {
            ReporterError::PdfGeneration("failed to aggregate canonical event amounts".to_string())
        })?;

    if scale == 0 {
        return Ok(sum.to_string());
    }

    let negative = sum < 0;
    let digits = sum.abs().to_string();
    let padded = if digits.len() <= scale {
        format!("{digits:0>width$}", width = scale + 1)
    } else {
        digits
    };
    let split = padded.len() - scale;
    let formatted = format!("{}.{}", &padded[..split], &padded[split..]);

    if negative {
        Ok(format!("-{formatted}"))
    } else {
        Ok(formatted)
    }
}

fn format_counterparty(event: &CanonicalEvent) -> String {
    match &event.counterparty.value {
        Some(value) if !value.trim().is_empty() => {
            format!("{:?} ({value})", event.counterparty.visibility)
        }
        _ => format!("{:?}", event.counterparty.visibility),
    }
}

fn summarize_evidence_refs(event: &CanonicalEvent) -> String {
    match event.evidence_refs.len() {
        0 => "none".to_string(),
        1 => event.evidence_refs[0].clone(),
        count => format!("{} refs, first {}", count, event.evidence_refs[0]),
    }
}

fn format_audit_period(
    audit_start: Option<DateTime<Utc>>,
    audit_end: Option<DateTime<Utc>>,
) -> String {
    match (audit_start, audit_end) {
        (Some(start), Some(end)) => format!(
            "{} to {}",
            start.format("%Y-%m-%d %H:%M:%S UTC"),
            end.format("%Y-%m-%d %H:%M:%S UTC")
        ),
        _ => "No canonical events available".to_string(),
    }
}

fn page_suffix(index: usize) -> String {
    if index == 0 {
        String::new()
    } else {
        format!(" (page {})", index + 1)
    }
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

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
