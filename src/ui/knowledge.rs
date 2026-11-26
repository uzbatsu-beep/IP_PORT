//! Read-only offline reference; catalog loading is shared with Discovery.
use super::preferences::Language;
use crate::knowledge_service::catalog;
use egui::Ui;

pub fn show(query: &mut String, language: Language, ui: &mut Ui) {
    ui.heading(language.text(
        "Industrial protocol reference",
        "Справочник промышленных протоколов",
    ));
    ui.label(language.text(
        "Reference only. Ports are not device identification.",
        "Справочник. Порт не подтверждает идентификацию устройства.",
    ));
    let Ok(data) = catalog() else {
        ui.label(language.text("Catalog unavailable", "Каталог недоступен"));
        return;
    };
    ui.label(format!("{} · {}", data.reviewed, data.entries.len()));
    ui.text_edit_singleline(query);
    let needle = query.trim().to_lowercase();
    for entry in &data.entries {
        if !needle.is_empty()
            && ![
                &entry.vendor,
                &entry.family,
                &entry.transport,
                &entry.ports,
                &entry.purpose,
                &entry.kind,
            ]
            .iter()
            .any(|value| value.to_lowercase().contains(&needle))
        {
            continue;
        }
        ui.push_id(&entry.id, |ui| {
            egui::CollapsingHeader::new(format!(
                "{} · {} · {}",
                entry.vendor, entry.family, entry.ports
            ))
            .show(ui, |ui| {
                ui.label(format!(
                    "{} · {} · {}",
                    entry.transport, entry.purpose, entry.kind
                ));
                ui.label(&entry.notes);
                ui.hyperlink_to(
                    language.text("Primary source", "Первоисточник"),
                    &entry.source,
                );
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_has_unique_ids_and_sources() {
        let data = catalog().as_ref().unwrap();
        let mut ids = std::collections::HashSet::new();
        assert!(data.entries.len() >= 40);
        for entry in &data.entries {
            assert!(ids.insert(&entry.id));
            assert!(entry.source.starts_with("https://"));
            assert!(!entry.kind.is_empty());
        }
        assert!(
            data.entries
                .iter()
                .any(|e| e.family.contains("NX100") && e.kind == "unverified_port")
        );
    }
}
