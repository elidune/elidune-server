use chrono::{DateTime, Utc};

use z3950_rs::marc_rs::record::{
    Agent, BibliographicLevel, Description, Indexing, Isbn as MarcIsbn, Item as MarcItem,
    LinkType, LinkedRecord, Local, Note, NoteType, Person, Publication, Record as MarcRecord,
    RecordStatus, RecordType, Relator, Responsibility, SeriesStatement, Subject, SubjectType,
    TargetAudience, Title,
};

use crate::{marc::MarcImportPreview, models::{
    Language, MediaType,
    author::{Author, Function},
    biblio::{AudienceType, Biblio, Collection, Edition, Isbn, Serie},
    item::Item,
}};

use std::str::FromStr;

impl From<z3950_rs::marc_rs::record::Relator> for Function {
    fn from(r: z3950_rs::marc_rs::record::Relator) -> Self {
        use z3950_rs::marc_rs::record::Relator as R;
        match r {
            R::Author => Function::Author,
            R::Illustrator => Function::Illustrator,
            R::Translator => Function::Translator,
            R::Editor => Function::ScientificAdvisor,
            R::PrefaceWriter => Function::PrefaceWriter,
            R::Photographer => Function::Photographer,
            R::Publisher => Function::PublishingDirector,
            R::Composer => Function::Composer,
            R::Other(_) => Function::Author,
        }
    }
}

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

/// Reverse of [`MediaType`] as derived from MARC [`RecordType`] in [`From<&RecordType> for MediaType`].
fn record_type_from_media_type(mt: &MediaType) -> RecordType {
    match mt {
        MediaType::PrintedText | MediaType::Comics | MediaType::Unknown | MediaType::All => {
            RecordType::LanguageMaterial
        }
        MediaType::Periodic => RecordType::LanguageMaterial,
        MediaType::Video | MediaType::VideoTape | MediaType::VideoDvd => {
            RecordType::ProjectedOrVideo
        }
        MediaType::Audio
        | MediaType::AudioNonMusic
        | MediaType::AudioNonMusicTape
        | MediaType::AudioNonMusicCd => RecordType::NonMusicalSound,
        MediaType::AudioMusic | MediaType::AudioMusicTape | MediaType::AudioMusicCd => {
            RecordType::NotatedMusic
        }
        MediaType::Multimedia | MediaType::CdRom => RecordType::ElectronicResource,
        MediaType::Images => RecordType::GraphicTwoDimensional,
    }
}

fn function_to_relator(f: Function) -> Relator {
    match f {
        Function::Author => Relator::Author,
        Function::Illustrator => Relator::Illustrator,
        Function::Translator => Relator::Translator,
        Function::ScientificAdvisor => Relator::Editor,
        Function::PrefaceWriter => Relator::PrefaceWriter,
        Function::Photographer => Relator::Photographer,
        Function::PublishingDirector => Relator::Publisher,
        Function::Composer => Relator::Composer,
    }
}

/// Reverse of [`From<z3950_rs::marc_rs::record::TargetAudience> for AudienceType`].
fn audience_type_to_target_audience(a: &AudienceType) -> TargetAudience {
    match a {
        AudienceType::Juvenile => TargetAudience::Juvenile,
        AudienceType::Preschool => TargetAudience::Preschool,
        AudienceType::Primary => TargetAudience::Primary,
        AudienceType::Children => TargetAudience::Children,
        AudienceType::YoungAdult => TargetAudience::YoungAdult,
        AudienceType::AdultSerious => TargetAudience::AdultSerious,
        AudienceType::Adult => TargetAudience::Adult,
        AudienceType::General => TargetAudience::General,
        AudienceType::Specialized => TargetAudience::Specialized,
        AudienceType::Unknown => TargetAudience::Unknown,
        AudienceType::Other(s) => TargetAudience::Other(s.clone()),
    }
}

fn author_to_marc_agent(author: &Author) -> Option<Agent> {
    let last = author
        .lastname
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let first = author
        .firstname
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let (name, forename) = match (last, first) {
        (Some(l), f_opt) => (l.to_string(), f_opt.map(|s| s.to_string())),
        (None, Some(f)) => (f.to_string(), None),
        (None, None) => return None,
    };
    Some(Agent::Person(Person {
        name,
        forename,
        dates: None,
        numeration: None,
        titles_associated: None,
        fuller_form: None,
        relator: author.function.map(function_to_relator),
    }))
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

impl From<z3950_rs::marc_rs::record::TargetAudience> for AudienceType {
    fn from(v: z3950_rs::marc_rs::record::TargetAudience) -> Self {
        use z3950_rs::marc_rs::record::TargetAudience as T;
        match v {
            T::Juvenile => AudienceType::Juvenile,
            T::Preschool => AudienceType::Preschool,
            T::Primary => AudienceType::Primary,
            T::Children => AudienceType::Children,
            T::YoungAdult => AudienceType::YoungAdult,
            T::AdultSerious => AudienceType::AdultSerious,
            T::Adult => AudienceType::Adult,
            T::General => AudienceType::General,
            T::Specialized => AudienceType::Specialized,
            T::Unknown => AudienceType::Unknown,
            T::Other(s) => AudienceType::Other(s),
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

// ── MarcRecord → Biblio ───────────────────────────────────────────────────────

impl From<MarcRecord> for Biblio {
    fn from(mut record: MarcRecord) -> Self {
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
                        function: person.relator.clone().map(Function::from),
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
        let audience_type: Option<AudienceType> = record.coded.target_audience.clone().map(AudienceType::from);

        // --- Series / collection from description / links ---
        let mut series_list: Vec<Serie> = Vec::new();
        let mut collection: Option<Collection> = None;

  

        // UNIMARC 410 → links.records[link_type=Series]: authority-controlled series.
        // Only used as fallback when no free-text series statement was found (225/490),
        // to avoid creating duplicates from the same bibliographic series.
        for link in record
            .links
            .records
            .iter()
            .filter(|l| matches!(l.link_type, Some(LinkType::Series)))
        {
            if let Some(title) = &link.title {
                series_list.push(Serie {
                    id: None,
                    key: None,
                    name: Some(title.clone()),
                    issn: link.issn.clone(),
                    created_at: None,
                    updated_at: None,
                    volume_number: link.volume.as_deref().and_then(extract_volume_number),
                });
            }
        }

        if series_list.is_empty() {
            // Series: UNIMARC 225 / MARC21 490 → description.series (free-text form on the document)
            for entry in &record.description.series {
                series_list.push(Serie {
                    id: None,
                    key: None,
                    name: Some(entry.title.clone()),
                    issn: entry.issn.clone(),
                    created_at: None,
                    updated_at: None,
                    volume_number: entry.volume.as_deref().and_then(extract_volume_number),
                });
            }
        }

        // Collection: UNIMARC 461 → links.records[link_type=SetLevel].
        // Represents the publisher collection (ensemble documentaire) the item belongs to.
        // No direct MARC21 equivalent is mapped in the current dictionary.
        if let Some(link) = record
            .links
            .records
            .iter()
            .find(|l| matches!(l.link_type, Some(LinkType::SetLevel)))
        {
            if let Some(title) = &link.title {
                collection = Some(Collection {
                    id: None,
                    key: None,
                    name: Some(title.clone()),
                    secondary_title: None,
                    tertiary_title: None,
                    issn: link.issn.clone(),
                    created_at: None,
                    updated_at: None,
                    volume_number: link.volume.as_deref().and_then(extract_volume_number),
                });
            }
        }

        // --- Physical items (from local MARC data) ---
        // and remove thoses from the record, we don't need them in the biblio
        let items: Vec<Item> = record.local.items.iter().map(Item::from).collect();
        record.local.items.clear();

        let collection_volume_numbers: Vec<Option<i16>> =
            collection.as_ref().map(|c| vec![c.volume_number]).unwrap_or_default();
        let collections_vec: Vec<Collection> = collection.into_iter().collect();

        Biblio {
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
            is_valid: Some(record.valid),
            series_ids: vec![],
            series_volume_numbers: series_list.iter().map(|s| s.volume_number).collect(),
            edition_id: None,
            collection_ids: vec![],
            collection_volume_numbers,
            created_at: None,
            updated_at: None,
            archived_at: None,
            authors,
            series: series_list,
            collections: collections_vec,
            edition,
            items,
            marc_record: Some(record),
        }
    }
}


impl From<MarcRecord> for MarcImportPreview {
    fn from(record: MarcRecord) -> Self {
        MarcImportPreview {
            validation_issues: record.validation_issues.clone(),
            biblio: Biblio::from(record).into(),
        }
    }
}

// ── Item (physical copy) mapping from MARC local data ────────────────────────

impl From<&MarcItem> for Item {
    fn from(s: &MarcItem) -> Self {
        let notes = match (&s.section, &s.document_type) {
            (Some(sec), Some(doc)) => Some(format!("{} — {}", sec, doc)),
            (Some(sec), None) => Some(sec.clone()),
            (None, Some(doc)) => Some(doc.clone()),
            (None, None) => None,
        };
        Item {
            id: None,
            biblio_id: None,
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
            borrowed: false,
        }
    }
}

// ── Biblio → MarcRecord ───────────────────────────────────────────────────────

impl From<&Biblio> for MarcRecord {
    fn from(item: &Biblio) -> Self {
        // If we already have a MARC record stored, just reuse it.
        if let Some(rec) = &item.marc_record {
            return rec.clone();
        }

        // Otherwise build a semantic record from relational data (mirrors [`From<MarcRecord> for Biblio`]).
        let mut record = MarcRecord::default();

        record.leader.status = RecordStatus::New;
        record.leader.record_type = record_type_from_media_type(&item.media_type);
        record.leader.bibliographic_level = if item.media_type == MediaType::Periodic {
            BibliographicLevel::Serial
        } else {
            BibliographicLevel::Monograph
        };

        if let Some(ref isbn) = item.isbn {
            let s = isbn.as_str().trim();
            if !s.is_empty() {
                record.identification.isbn.push(MarcIsbn {
                    value: s.to_string(),
                    qualifying: None,
                });
            }
        }

        let agents: Vec<Agent> = item
            .authors
            .iter()
            .filter_map(author_to_marc_agent)
            .collect();
        if !agents.is_empty() {
            let mut it = agents.into_iter();
            record.responsibility = Responsibility {
                main_entry: it.next(),
                added_entries: it.collect(),
            };
        }

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

        // Series (UNIMARC 225 / MARC21 490) — same source as [`From<MarcRecord>`] uses for `Serie`.
        for s in &item.series {
            if let Some(ref title) = s.name {
                if title.is_empty() {
                    continue;
                }
                record.description.series.push(SeriesStatement {
                    title: title.clone(),
                    volume: s.volume_number.map(|v| v.to_string()),
                    issn: s.issn.clone(),
                });
            }
        }

        // Collections (e.g. UNIMARC 461) — [`LinkType::SetLevel`], same as import fallback for `Collection`.
        for c in &item.collections {
            let has_title = c.name.as_ref().map_or(false, |t| !t.is_empty());
            if !has_title && c.id.is_none() && c.key.as_ref().map_or(true, |k| k.is_empty()) {
                continue;
            }
            let identifier = c
                .id
                .map(|id| id.to_string())
                .or_else(|| c.key.clone())
                .or_else(|| c.name.clone())
                .unwrap_or_else(|| "collection".to_string());
            record.links.records.push(LinkedRecord {
                link_type: Some(LinkType::SetLevel),
                identifier,
                title: c.name.clone(),
                edition: None,
                qualifier: None,
                issn: c.issn.clone(),
                volume: c.volume_number.map(|v| v.to_string()),
                relationship_info: None,
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

        if let Some(ref aud) = item.audience_type {
            record.coded.target_audience = Some(audience_type_to_target_audience(aud));
        }

        record.valid = item.is_valid.unwrap_or(true);

        // Local items (physical copies)
        record.local = Local {
            items: biblio_items_to_marc_items(&item.items, None, None, None),
        };

        record
    }
}

/// Maps catalog [`Item`] rows to marc-rs local [`MarcItem`] entries.
///
/// When `loan_start` / `loan_expiry` / `returned_at` are provided (export with loan context),
/// `loan_date` and `return_date` are filled (ISO 8601 dates `YYYY-MM-DD`). For active loans,
/// `return_date` is the due date (`loan_expiry`); when the loan is returned, it is the actual
/// return date (`returned_at`).
pub fn biblio_items_to_marc_items(
    items: &[Item],
    loan_start: Option<DateTime<Utc>>,
    loan_expiry: Option<DateTime<Utc>>,
    returned_at: Option<DateTime<Utc>>,
) -> Vec<MarcItem> {
    let loan_date = loan_start.map(|d| d.format("%Y-%m-%d").to_string());
    let return_date = match returned_at {
        Some(d) => Some(d.format("%Y-%m-%d").to_string()),
        None => loan_expiry.map(|d| d.format("%Y-%m-%d").to_string()),
    };
    items
        .iter()
        .map(|s| MarcItem {
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
            loan_date: loan_date.clone(),
            return_date: return_date.clone(),
            acquisition_date: None,
            item_type: None,
            record_control_number: None,
            document_type: s.notes.clone(),
            circulation_status: None,
        })
        .collect()
}

/// Builds a [`MarcRecord`] for loan export: uses stored `biblio.marc_record` when present
/// (bibliographic notice without local items), otherwise [`MarcRecord::from`] the relational
/// biblio. Always sets `local.items` to the borrowed copy(ies) in `biblio.items`, with loan dates.
pub fn marc_record_for_loan_export(
    biblio: &Biblio,
    loan_start: DateTime<Utc>,
    loan_expiry: DateTime<Utc>,
    returned_at: Option<DateTime<Utc>>,
) -> MarcRecord {
    let mut record = match &biblio.marc_record {
        Some(rec) => rec.clone(),
        None => MarcRecord::from(biblio),
    };
    record.local.items = biblio_items_to_marc_items(
        &biblio.items,
        Some(loan_start),
        Some(loan_expiry),
        returned_at,
    );
    record
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
