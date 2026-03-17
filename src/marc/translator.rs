

use z3950_rs::marc_rs::record::{
    Agent, BibliographicLevel, Description, Indexing, Local, Note, NoteType,
    Publication, Record as MarcRecord, RecordStatus, RecordType,
    Specimen as MarcSpecimen, Subject, SubjectType, Title,
};

use crate::models::{
    MediaType, author::Author, item::{Collection, Edition, Isbn, Item, Language, Serie}, specimen::Specimen
};

// ── Helpers (local) ──────────────────────────────────────────────────────────

/// Parse "vol. 5", "tome 12", "no. 3", or bare "5" → Some(5). Returns None if no digit found.
fn extract_volume_number(s: &str) -> Option<i16> {
    let s = s.trim();
    if let Ok(n) = s.parse::<i16>() {
        return Some(n);
    }
    s.split_whitespace()
        .find_map(|word| {
            let digits: String = word.chars().filter(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
}



impl From<&RecordType> for MediaType {
    fn from(rt: &RecordType) -> Self {
        match rt {
            RecordType::LanguageMaterial => MediaType::PrintedText,
            RecordType::NotatedMusic => MediaType::AudioMusic,
            RecordType::PrintedCartographic => MediaType::CdRom,
            RecordType::ManuscriptText => MediaType::Multimedia,
            RecordType::ProjectedOrVideo => MediaType::Video,
            RecordType::NonMusicalSound => MediaType::Audio,
            RecordType::MusicalSound => MediaType::AudioMusic,
            RecordType::GraphicTwoDimensional => MediaType::Images,
            RecordType::ElectronicResource => MediaType::Multimedia,
            RecordType::MixedMaterials => MediaType::Unknown,
            _ => MediaType::Unknown,
        }
    }
}

impl From<&z3950_rs::marc_rs::record::Language> for Language {
    fn from(l: &z3950_rs::marc_rs::record::Language) -> Self {
        match l {
            z3950_rs::marc_rs::record::Language::French => Language::French,
            z3950_rs::marc_rs::record::Language::English => Language::English,
            z3950_rs::marc_rs::record::Language::German => Language::German,
            z3950_rs::marc_rs::record::Language::Spanish => Language::Spanish,
            z3950_rs::marc_rs::record::Language::Italian => Language::Italian,
            z3950_rs::marc_rs::record::Language::Portuguese => Language::Portuguese,
            z3950_rs::marc_rs::record::Language::Japanese => Language::Japanese,
            z3950_rs::marc_rs::record::Language::Chinese => Language::Chinese,
            z3950_rs::marc_rs::record::Language::Russian => Language::Russian,
            z3950_rs::marc_rs::record::Language::Arabic => Language::Arabic,
            z3950_rs::marc_rs::record::Language::Dutch => Language::Dutch,
            z3950_rs::marc_rs::record::Language::Swedish => Language::Swedish,
            z3950_rs::marc_rs::record::Language::Norwegian => Language::Norwegian,
            z3950_rs::marc_rs::record::Language::Danish => Language::Danish,
            z3950_rs::marc_rs::record::Language::Finnish => Language::Finnish,
            z3950_rs::marc_rs::record::Language::Polish => Language::Polish,
            z3950_rs::marc_rs::record::Language::Czech => Language::Czech,
            z3950_rs::marc_rs::record::Language::Hungarian => Language::Hungarian,
            z3950_rs::marc_rs::record::Language::Romanian => Language::Romanian,
            z3950_rs::marc_rs::record::Language::Turkish => Language::Turkish,
            z3950_rs::marc_rs::record::Language::Korean => Language::Korean,
            z3950_rs::marc_rs::record::Language::Latin => Language::Latin,
            z3950_rs::marc_rs::record::Language::Greek => Language::Greek,
            z3950_rs::marc_rs::record::Language::Croatian => Language::Croatian,
            z3950_rs::marc_rs::record::Language::Hindi => Language::Hindi,
            z3950_rs::marc_rs::record::Language::Hebrew => Language::Hebrew,
            z3950_rs::marc_rs::record::Language::Persian => Language::Persian,
            z3950_rs::marc_rs::record::Language::Catalan => Language::Catalan,
            z3950_rs::marc_rs::record::Language::Thai => Language::Thai,
            z3950_rs::marc_rs::record::Language::Vietnamese => Language::Vietnamese,
            z3950_rs::marc_rs::record::Language::Indonesian => Language::Indonesian,
            z3950_rs::marc_rs::record::Language::Malay => Language::Malay,
            z3950_rs::marc_rs::record::Language::Other(_) => Language::Unknown,
        }
    }
}

impl From<Language> for z3950_rs::marc_rs::record::Language {
    fn from(l: Language) -> Self {
        match l {
            Language::French => z3950_rs::marc_rs::record::Language::French,
            Language::English => z3950_rs::marc_rs::record::Language::English,
            Language::German => z3950_rs::marc_rs::record::Language::German,
        
            Language::Spanish => z3950_rs::marc_rs::record::Language::Spanish,
            Language::Italian => z3950_rs::marc_rs::record::Language::Italian,
            Language::Portuguese => z3950_rs::marc_rs::record::Language::Portuguese,
            Language::Japanese => z3950_rs::marc_rs::record::Language::Japanese,
            Language::Chinese => z3950_rs::marc_rs::record::Language::Chinese,
            Language::Russian => z3950_rs::marc_rs::record::Language::Russian,
            Language::Arabic => z3950_rs::marc_rs::record::Language::Arabic,
            Language::Dutch => z3950_rs::marc_rs::record::Language::Dutch,
            Language::Swedish => z3950_rs::marc_rs::record::Language::Swedish,
            Language::Norwegian => z3950_rs::marc_rs::record::Language::Norwegian,
            Language::Danish => z3950_rs::marc_rs::record::Language::Danish,
            Language::Finnish => z3950_rs::marc_rs::record::Language::Finnish,
            Language::Polish => z3950_rs::marc_rs::record::Language::Polish,
            Language::Czech => z3950_rs::marc_rs::record::Language::Czech,
    
            Language::Hungarian => z3950_rs::marc_rs::record::Language::Hungarian,
            Language::Romanian => z3950_rs::marc_rs::record::Language::Romanian,
            Language::Turkish => z3950_rs::marc_rs::record::Language::Turkish,
            Language::Korean => z3950_rs::marc_rs::record::Language::Korean,
            Language::Latin => z3950_rs::marc_rs::record::Language::Latin,
            Language::Greek => z3950_rs::marc_rs::record::Language::Greek,
            Language::Croatian => z3950_rs::marc_rs::record::Language::Croatian,

            Language::Hindi => z3950_rs::marc_rs::record::Language::Hindi,
            Language::Hebrew => z3950_rs::marc_rs::record::Language::Hebrew,
            Language::Persian => z3950_rs::marc_rs::record::Language::Persian,
            Language::Catalan => z3950_rs::marc_rs::record::Language::Catalan,
            Language::Thai => z3950_rs::marc_rs::record::Language::Thai,
            Language::Vietnamese => z3950_rs::marc_rs::record::Language::Vietnamese,
            Language::Indonesian => z3950_rs::marc_rs::record::Language::Indonesian,
            Language::Malay => z3950_rs::marc_rs::record::Language::Malay,
            Language::Unknown => z3950_rs::marc_rs::record::Language::Other(String::new()),
        }
    }
}

#[allow(dead_code)]
fn sync_note<F>(notes: &mut Vec<Note>, matcher: F, new_note: Note)
where
    F: Fn(&Note) -> bool,
{
    if let Some(pos) = notes.iter().position(|n| matcher(n)) {
        notes[pos] = new_note;
    } else {
        notes.push(new_note);
    }
}

#[allow(dead_code)]
fn remove_notes<F>(notes: &mut Vec<Note>, matcher: F)
where
    F: Fn(&Note) -> bool,
{
    notes.retain(|n| !matcher(n));
}

// ── MarcRecord → Item ─────────────────────────────────────────────────────────

impl From<MarcRecord> for Item {
    fn from(record: MarcRecord) -> Self {
        // --- ISBN ---
        // Requires `record.isbn_string()` in marc-rs (see module doc).
        let isbn = record.isbn_string().map(Isbn::new).filter(|i| !i.is_empty());

        // --- Title ---
        let title = record.title_main().map(|s| s.to_string());

        // --- Media type ---
        let media_type = MediaType::from(&record.leader.record_type);

        // --- Authors: personal entries only ---
        let authors: Vec<Author> = record
            .authors()
            .into_iter()
            .filter_map(|a| 
                match a {
                    Agent::Person(person) => Some(Author{
                        id: 0,
                        key: None,
                        lastname: Some(person.name.clone()),
                        firstname: person.forename.clone(),
                        bio: None,
                        notes: None,
                        function: person.relator.clone(),
                    }),
                    _ => None,
                })
            .collect();

        // --- Subject / keywords ---
        let subject = record.subject_main().map(|s| s.to_string());
        let kws = record.keywords();
        let keywords = if kws.is_empty() { None } else { Some(kws.to_vec()) };

        // --- Edition info / publication date ---
        let publication_date = record.publication_date().map(|s| s.to_string());

        let first_pub: Option<&Publication> = {
            let Description { publication, .. } = &record.description;
            publication.first()
        };

        let edition = first_pub.map(|p| Edition {
            id: None,
            publisher_name: p.publisher.clone(),
            place_of_publication: p.place.clone(),
            date: p.date.clone(),
            created_at: None,
            updated_at: None,
        });

        // --- Physical description ---
        let page_extent = record.page_extent().map(|s| s.to_string());
        let format = record.dimensions().map(|s| s.to_string());
        let accompanying_material = record.accompanying_material_text().map(|s| s.to_string());

        // --- Notes ---
        let table_of_contents = record.table_of_contents_text().map(|s| s.to_string());
        let abstract_ = record.abstract_text().map(|s| s.to_string());
        let notes = record.general_note_text().map(|s| s.to_string());

        // --- Language ---
        let lang = record.lang_primary().map(Into::into);
        let lang_orig = record.lang_original().map(Language::from);

        // --- Audience type ---
        let audience_type: Option<i16> = match record.coded.target_audience {
            Some(z3950_rs::marc_rs::record::TargetAudience::Juvenile) => Some(106),
            Some(z3950_rs::marc_rs::record::TargetAudience::General) => Some(97),
            _ => None,
        };

        // --- Series / collection from description / links ---
        let mut serie: Option<Serie> = None;
        let mut serie_vol_number: Option<i16> = None;
        let mut collection: Option<Collection> = None;
        let mut collection_vol_number: Option<i16> = None;

        // Series from description.series
        if let Some(first_series) = record.description.series.first() {
            serie_vol_number = first_series
                .volume
                .as_deref()
                .and_then(extract_volume_number);

            serie = Some(Serie {
                id: None,
                key: None,
                name: Some(first_series.title.clone()),
                issn: first_series.issn.clone(),
                created_at: None,
                updated_at: None,
            });
        }

        // Collection from links.records (first with title)
        if let Some(link) = record.links.records.first() {
            if let Some(title) = &link.title {
                collection_vol_number = link
                    .volume
                    .as_deref()
                    .and_then(extract_volume_number);

                collection = Some(Collection {
                    id: None,
                    key: None,
                    primary_title: Some(title.clone()),
                    secondary_title: None,
                    tertiary_title: None,
                    issn: link.issn.clone(),
                    created_at: None,
                    updated_at: None,
                });
            }
        }

        // --- Specimens ---
        let specimens: Vec<Specimen> = record.local.specimens.iter().map(Specimen::from).collect();

        Item {
            id: None,
            media_type,
            isbn,
            title,
            subject,
            audience_type,
            lang,
            lang_orig,
            publication_date,
            page_extent,
            format,
            table_of_contents,
            accompanying_material,
            abstract_,
            notes,
            keywords,
            is_valid: Some(1),
            series_id: None,
            series_volume_number: serie_vol_number,
            edition_id: None,
            collection_id: None,
            collection_sequence_number: None,
            collection_volume_number: collection_vol_number,
            created_at: None,
            updated_at: None,
            archived_at: None,
            authors,
            series: serie,
            collection,
            edition,
            specimens,
            marc_record: Some(record),
        }
    }
}

// ── Specimen mapping ──────────────────────────────────────────────────────────

impl From<&MarcSpecimen> for Specimen {
    fn from(s: &MarcSpecimen) -> Self {
        let notes = match (&s.section, &s.document_type) {
            (Some(sec), Some(doc)) => Some(format!("{} — {}", sec, doc)),
            (Some(sec), None) => Some(sec.clone()),
            (None, Some(doc)) => Some(doc.clone()),
            (None, None) => None,
        };
        Specimen {
            id: None,
            item_id: None,
            source_id: None,
            barcode: s.barcode.clone(),
            call_number: s.call_number.clone(),
            volume_designation: None,
            place: None,
            borrowable: true,
            circulation_status: None,
            notes,
            price: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
            source_name: s.library.clone(),
            availability: Some(0),
        }
    }
}

// ── Item → MarcRecord ─────────────────────────────────────────────────────────

impl From<&Item> for MarcRecord {
    fn from(item: &Item) -> Self {
        // If we already have a MARC record stored, just reuse it.
        if let Some(rec) = &item.marc_record {
            return rec.clone();
        }

        // Otherwise build a minimal record.
        let mut record = MarcRecord::default();

        record.leader.status = RecordStatus::New;
        record.leader.record_type = RecordType::LanguageMaterial;
        record.leader.bibliographic_level = BibliographicLevel::Monograph;

        // Title
        if let Some(ref title) = item.title {
            record.description.title = Some(Title {
                main: title.clone(),
                subtitle: None,
                parallel: Vec::new(),
                responsibility: None,
                medium: None,
                number_of_part: None,
                name_of_part: None,
            });
        }

        // Publication
        if item.edition.is_some() || item.publication_date.is_some() {
            let (place, publisher, date) = if let Some(ref ed) = item.edition {
                (
                    ed.place_of_publication.clone(),
                    ed.publisher_name.clone(),
                    ed.date.clone(),
                )
            } else {
                (None, None, item.publication_date.clone())
            };

            record.description.publication = vec![Publication {
                place,
                publisher,
                date,
                function: None,
                manufacture_place: None,
                manufacturer: None,
                manufacture_date: None,
            }];
        }

        // Physical description
        if item.page_extent.is_some() || item.format.is_some() || item.accompanying_material.is_some()
        {
            record.description.physical_description =
                Some(z3950_rs::marc_rs::record::PhysicalDescription {
                    extent: item.page_extent.clone(),
                    other_physical_details: None,
                    dimensions: item.format.clone(),
                    accompanying_material: item.accompanying_material.clone(),
                });
        }

        // Notes (only General / Contents / Summary)
        record.notes.items.clear();
        if let Some(ref text) = item.notes {
            record.notes.items.push(Note {
                note_type: Some(NoteType::General),
                text: text.clone(),
            });
        }
        if let Some(ref text) = item.table_of_contents {
            record.notes.items.push(Note {
                note_type: Some(NoteType::Contents),
                text: text.clone(),
            });
        }
        if let Some(ref text) = item.abstract_ {
            record.notes.items.push(Note {
                note_type: Some(NoteType::Summary),
                text: text.clone(),
            });
        }

        // Subjects and keywords
        record.indexing = Indexing::default();
        if let Some(ref subject) = item.subject {
            record.indexing.subjects.push(Subject {
                heading_type: SubjectType::Topical,
                value: subject.clone(),
            });
        }
        if let Some(ref keywords) = item.keywords {
            for kw in keywords {
                if !kw.is_empty() {
                    record.indexing.uncontrolled_terms.push(kw.clone());
                }
            }
        }

        // Languages
        if let Some(ref lang) = item.lang {
            record.coded.languages.push((*lang).into());
        }
        if let Some(ref lang_orig) = item.lang_orig {
            record.coded.original_languages.push((*lang_orig).into());
        }

        // Local specimens
        record.local = Local {
            specimens: item
                .specimens
                .iter()
                .map(|s| {
                    MarcSpecimen {
                        library: s.source_name.clone(),
                        sub_library: None,
                        section: None,
                        section_code: None,
                        level_code: None,
                        barcode: s.barcode.clone(),
                        call_number: s.call_number.clone(),
                        inventory_number: None,
                        creation_date: None,
                        modification_date: None,
                        loan_date: None,
                        return_date: None,
                        acquisition_date: None,
                        item_type: None,
                        record_control_number: None,
                        document_type: s.notes.clone(),
                        circulation_status: None,
                    }
                })
                .collect(),
        };

        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_volume_number() {
        assert_eq!(extract_volume_number("1"), Some(1));
        assert_eq!(extract_volume_number("vol. 5"), Some(5));
        assert_eq!(extract_volume_number("tome 12"), Some(12));
        assert_eq!(extract_volume_number("no. 3"), Some(3));
        assert_eq!(extract_volume_number("abc"), None);
        assert_eq!(extract_volume_number(""), None);
    }
}
