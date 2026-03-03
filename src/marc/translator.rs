//! MARC to Item translator
//!
//! Translates MARC records (UNIMARC or MARC21) into the internal Item structure
//! using marc-rs typed field structures.
//!
//! Field values in marc-rs may be enums (e.g. Leader positions, indicators);
//! use `.into()` or `char::from()` / library conversion to get char or int when needed.

use z3950_rs::marc_rs::fields::common::NoteData;
use z3950_rs::marc_rs::fields::*;
use z3950_rs::marc_rs::{MarcFormat, Record as MarcRecord};

use crate::models::{
    author::Author,
    item::{Collection, Edition, Item, Serie},
    specimen::Specimen,
};

/// Detect MARC format from record control fields (008 = MARC21, 009 = UNIMARC).
fn detect_format(record: &MarcRecord) -> MarcFormat {
    let has_008 = record
        .control()
        .iter()
        .any(|c| matches!(c, Control::FixedLengthDataElements(_)));
    let has_009 = record
        .control()
        .iter()
        .any(|c| matches!(c, Control::LocalControlNumber(_)));
    if has_009 && !has_008 {
        MarcFormat::Unimarc
    } else {
        MarcFormat::Marc21
    }
}

/// Get first subfield value from other_data by tag and code.
fn other_subfield(record: &MarcRecord, tag: &str, code: char) -> Option<String> {
    record
        .other_data()
        .iter()
        .find(|f| f.tag == tag)
        .and_then(|f| {
            f.subfields
                .iter()
                .find(|sf| sf.code == code)
                .map(|sf| sf.value.clone())
        })
}

/// Get all subfield values from other_data by tag and code.
fn other_subfields_all(record: &MarcRecord, tag: &str, code: char) -> Vec<String> {
    record
        .other_data()
        .iter()
        .filter(|f| f.tag == tag)
        .flat_map(|f| {
            f.subfields
                .iter()
                .filter(|sf| sf.code == code)
                .map(|sf| sf.value.clone())
        })
        .collect()
}

/// Get value from Control::FixedLengthDataElements (008).
fn control_008(record: &MarcRecord) -> Option<&str> {
    record
        .control()
        .iter()
        .find_map(|c| match c {
            Control::FixedLengthDataElements(_) => Some(c.value()),
            _ => None,
        })
}

/// Get value from other_control by tag (e.g. UNIMARC 100).
fn other_control_value<'a>(record: &'a MarcRecord, tag: &str) -> Option<&'a str> {
    record
        .other_control()
        .iter()
        .find(|f| f.tag == tag)
        .map(|f| f.value.as_str())
}

fn normalize_isbn(isbn: &str) -> String {
    isbn.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn clean_title(title: &str) -> String {
    title
        .trim_end_matches(|c| c == '/' || c == ':' || c == ';' || c == ' ')
        .to_string()
}

fn clean_publisher(publisher: &str) -> String {
    publisher.trim_end_matches(',').trim().to_string()
}

fn clean_place(place: &str) -> String {
    place
        .trim_end_matches(|c| c == ':' || c == ';' || c == ' ')
        .to_string()
}

/// Parse author name in "Lastname, Firstname" format (MARC21 100/700).
fn parse_author_name(name: &str) -> (String, Option<String>) {
    if let Some(pos) = name.find(',') {
        let lastname = name[..pos].trim().to_string();
        let firstname = name[pos + 1..].trim().to_string();
        (
            lastname,
            if firstname.is_empty() {
                None
            } else {
                Some(firstname)
            },
        )
    } else {
        (name.trim().to_string(), None)
    }
}

fn extract_volume_number(vol: &str) -> Option<i16> {
    vol.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn language_code_to_id(code: &str) -> Option<i16> {
    match code.to_lowercase().as_str() {
        "fre" | "fra" => Some(1),
        "eng" => Some(2),
        "ger" | "deu" => Some(3),
        "jpn" => Some(4),
        "spa" => Some(5),
        "por" => Some(6),
        _ => Some(0),
    }
}

/// Get record type as char (Leader position 6). Works whether marc-rs uses char or RecordType enum.
fn record_type_as_char(record: &MarcRecord) -> char {
    let rt = record.leader().record_type;
    char::from(rt)
}

/// Audience from MARC21 008 position 22.
fn audience_marc21(record: &MarcRecord) -> Option<i16> {
    control_008(record).and_then(|cf| {
        if cf.len() >= 23 {
            cf.chars().nth(22)
        } else {
            None
        }
    }).map(|c| match c {
        'a' | 'b' | 'c' | 'd' | 'j' => 106,
        _ => 97,
    })
}

/// Audience from UNIMARC 100$a position 17 (via other_control "100").
fn audience_unimarc(record: &MarcRecord) -> Option<i16> {
    other_control_value(record, "100").and_then(|cf| {
        if cf.len() >= 18 {
            cf.chars().nth(17)
        } else {
            None
        }
    }).map(|c| match c {
        'a' | 'b' | 'c' | 'd' | 'e' => 106,
        _ => 97,
    })
}

/// Get note text from NoteData (first $a).
fn note_text(d: &NoteData) -> &str {
    d.text.as_str()
}

impl From<MarcRecord> for Item {
    fn from(record: MarcRecord) -> Self {
        let format = detect_format(&record);
        let is_marc21 = format == MarcFormat::Marc21;

        // Price: first typed ISBN price_or_acquisition (020$c MARC21, 010$d UNIMARC)
        let price = record
            .isbns()
            .first()
            .and_then(|i| i.price_or_acquisition.clone());

        // ISBN: from typed isbns() first, then fallback to other_data 020/010
        let isbn = record
                .isbns()
                .iter()
                .map(|i| i.sanitized_number())
                .filter(|n| !n.is_empty())
                .next();


        // Title: first TitleStatement; then normalize with `clean_title` for all formats.
        let raw_title = record
            .titles()
            .first()
            .and_then(|t| match t {
                Title::TitleStatement(d) => Some(d.title.clone()),
                _ => None,
            })
            .unwrap_or_default();

        // Authors: semantic authors from marc-rs (main + added entries)
        let authors = record
            .authors()
            .iter()
        .map(|a| Author {
            id: 0,
            key: None,
            lastname: a.last_name.clone(),
            firstname: a.first_name.clone(),
            bio: None,
            notes: None,
            function: a.function.clone(),
        })
        .collect();

        // Edition and publication: from record.edition_info() (250/260/264 MARC21, 205/210 UNIMARC)
        let edition_info = record.edition_info();
        let place = edition_info.place.map(|s| clean_place(&s));
        let publisher = edition_info.publisher.map(|s| clean_publisher(&s));
        let publication_date = edition_info.date.map(String::from);
        let edition = publisher.map(|name| Edition {
            id: None,
            publisher_name: Some(name),
            place_of_publication: place,
            date: publication_date.clone(),
            created_at: None,
            updated_at: None,
        });

        // Serie: build from series fields that represent a narrative/author series,
        // following COLLECTION_SERIE_MAPPING.md.
        let mut serie: Option<Serie> = None;
        let mut serie_vol_number: Option<i16> = None;

        for s in record.series() {
            match s {
                // Author / corporate / meeting series: prefer these as true series names.
                Series::SeriesPersonalName(d) | Series::SeriesAddedEntryPersonalName(d) => {
                    if serie.is_none() {
                        let name = d.name.clone();
                        if !name.is_empty() {
                            serie = Some(Serie {
                                id: None,
                                key: None,
                                name: Some(name),
                                issn: None,
                                created_at: None,
                                updated_at: None,
                            });
                        }
                    }
                }
                Series::SeriesCorporateName(d) | Series::SeriesAddedEntryCorporateName(d) => {
                    if serie.is_none() {
                        let name = d.name.clone();
                        if !name.is_empty() {
                            serie = Some(Serie {
                                id: None,
                                key: None,
                                name: Some(name),
                                issn: None,
                                created_at: None,
                                updated_at: None,
                            });
                        }
                    }
                }
                Series::SeriesMeetingName(d) | Series::SeriesAddedEntryMeetingName(d) => {
                    if serie.is_none() {
                        let name = d.name.clone();
                        if !name.is_empty() {
                            serie = Some(Serie {
                                id: None,
                                key: None,
                                name: Some(name),
                                issn: None,
                                created_at: None,
                                updated_at: None,
                            });
                        }
                    }
                }
                // Statement / title treated as Serie when there is a volume number but no ISSN.
                Series::SeriesStatement(d) | Series::SeriesTitle(d) => {
                    if serie.is_none() && d.issn.is_none() {
                        if let Some(vol_raw) = d.volume.as_deref() {
                            if let Some(vol) = extract_volume_number(vol_raw) {
                                serie_vol_number = Some(vol);
                                serie = Some(Serie {
                                    id: None,
                                    key: None,
                                    name: Some(d.statement.clone()),
                                    issn: d.issn.clone(),
                                    created_at: None,
                                    updated_at: None,
                                });
                            }
                        }
                    }
                }
                // Uniform title is used for Collection (830), not Serie, so we skip it here.
                Series::SeriesUniformTitle(_) => {}
            }
        }

        // Collection: editorial collections with ISSN, or 410/411 links as fallback.
        let mut collection: Option<Collection> = None;
        let mut collection_vol_number: Option<i16> = None;

        // Prefer series statements/titles that look like editorial collections (have ISSN).
        if let Some((d, vol)) = record
            .series()
            .iter()
            .find_map(|s| match s {
                // 830: best candidate for collection primary_title.
                Series::SeriesUniformTitle(d) => {
                    let vol = d
                        .volume
                        .as_deref()
                        .and_then(extract_volume_number);
                    Some((d, vol))
                }
                // 225/440/490 with ISSN: editorial collections.
                Series::SeriesStatement(d) | Series::SeriesTitle(d) if d.issn.is_some() => {
                    let vol = d
                        .volume
                        .as_deref()
                        .and_then(extract_volume_number);
                    Some((d, vol))
                }
                _ => None,
            })
        {
            collection = Some(Collection {
                id: None,
                key: None,
                primary_title: Some(d.statement.clone()),
                secondary_title: d.subseries.clone(),
                tertiary_title: None,
                issn: d.issn.clone(),
                created_at: None,
                updated_at: None,
            });
            collection_vol_number = vol;
        } else {
            // Fallback: UNIMARC 410/411 linking information.
            let title = record
                .linking()
                .iter()
                .find_map(|l| {
                    if let Linking::MainSeriesEntry(d) = l {
                        d.title.clone()
                    } else {
                        None
                    }
                });

            if let Some(t) = title {
                collection = Some(Collection {
                    id: None,
                    key: None,
                    primary_title: Some(t),
                    secondary_title: None,
                    tertiary_title: None,
                    issn: other_subfield(&record, "410", 'x'),
                    created_at: None,
                    updated_at: None,
                });
                collection_vol_number = other_subfield(&record, "410", 'v')
                    .and_then(|v| extract_volume_number(&v));
            }
        }

        // Physical: extent, dimensions, accompanying
        let (nb_pages, format, addon) = record
            .physical()
            .iter()
            .find_map(|p| {
                if let Physical::PhysicalDescription(ref d) = p {
                    Some((
                        Some(d.extent.clone()),
                        d.dimensions.clone(),
                        d.accompanying_material.clone(),
                    ))
                } else {
                    None
                }
            })
            .unwrap_or((None, None, None));

        // Notes: Summary -> abstract_, GeneralNote -> notes, FormattedContentsNote (505) -> content
        let abstract_ = record.notes().iter().find_map(|n| {
            if let Note::Summary(d) = n {
                Some(note_text(d).to_string())
            } else {
                None
            }
        });
        let notes = record.notes().iter().find_map(|n| {
            if let Note::GeneralNote(d) = n {
                Some(note_text(d).to_string())
            } else {
                None
            }
        });
        let content = record.notes().iter().find_map(|n| {
            if let Note::FormattedContentsNote(d) = n {
                Some(note_text(d).to_string())
            } else {
                None
            }
        });

        // Subjects: topical term -> subject, uncontrolled -> keywords
        let subject = record.subjects().iter().find_map(|s| {
            if let Subject::SubjectTopicalTerm(d) = s {
                Some(d.term.clone())
            } else if let Subject::IndexTermUncontrolled(_) = s {
                None
            } else {
                None
            }
        });
        let keywords = {
            let kws: Vec<String> = record
                .subjects()
                .iter()
                .filter_map(|s| {
                    if let Subject::IndexTermUncontrolled(d) = s {
                        Some(d.term.clone())
                    } else if let Subject::SubjectTopicalTerm(d) = s {
                        Some(d.term.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if kws.is_empty() {
                None
            } else {
                Some(kws.join(", "))
            }
        };
        let subject = subject.or_else(|| keywords.clone());

        // Call number: from Dewey classification (082/676) or other sources
        let call_number = record
            .dewey()
            .map(String::from)
            .or_else(|| {
                let dewey_tag = if is_marc21 { "082" } else { "676" };
                other_subfield(&record, dewey_tag, 'a')
            });

        // Language: record.language_codes() first, then 008 (MARC21) or other_control 101 (UNIMARC)
        let lang_codes = record.language_codes();
        let lang = lang_codes
            .first()
            .and_then(|lc| language_code_to_id(lc.as_code()))
            .or_else(|| {
                if is_marc21 {
                    control_008(&record)
                        .and_then(|cf| if cf.len() >= 38 { Some(&cf[35..38]) } else { None })
                        .and_then(language_code_to_id)
                } else {
                    other_subfield(&record, "101", 'a')
                        .as_ref()
                        .and_then(|s| language_code_to_id(s))
                }
            });
        let lang_orig = lang_codes
            .get(1)
            .and_then(|lc| language_code_to_id(lc.as_code()));

      
        let media_type = crate::repository::items::record_type_to_media_type_db(
            record_type_as_char(&record),
        );
        let public_type = if is_marc21 {
            audience_marc21(&record)
        } else {
            audience_unimarc(&record)
        };



        Item {
            id: None,
            media_type: Some(media_type),
            isbn,
            barcode: None,
            call_number,
            price,
            title: Some(clean_title(&raw_title)),
            genre: None,
            subject,
            audience_type: public_type,
            lang,
            lang_orig,
            publication_date,
            page_extent: nb_pages,
            format,
            table_of_contents: content,
            accompanying_material: addon,
            abstract_,
            notes,
            keywords,
            state: None,
            is_valid: Some(1),
            series_id: None,
            series_volume_number: serie_vol_number,
            edition_id: None,
            collection_id: None,
            collection_sequence_number: None,
            collection_volume_number: collection_vol_number,
            status: 0,
            created_at: None,
            updated_at: None,
            archived_at: None,
            authors,
            series: serie,
            collection,
            edition,
            specimens: record
                .specimens()
                .iter()
                .map(marc_specimen_to_specimen)
                .collect(),
            marc_record: Some(serde_json::to_value(&record).unwrap()),
        }
    }
}

/// Map marc_rs specimen (995/952) to our Specimen model (preview, id=0).
fn marc_specimen_to_specimen(s: &z3950_rs::marc_rs::fields::Specimen) -> Specimen {
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
        borrow_status: Some(98),
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

fn language_id_to_code(id: i16) -> Option<&'static str> {
    match id {
        1 => Some("fre"),
        2 => Some("eng"),
        3 => Some("ger"),
        4 => Some("jpn"),
        5 => Some("spa"),
        6 => Some("por"),
        0 => Some("und"),
        _ => None,
    }
}

fn make_note(text: String) -> NoteData {
    NoteData { text, other_subfields: Vec::new() }
}

fn make_subject(term: String) -> SubjectData {
    SubjectData {
        thesaurus: SubjectThesaurus::default(), term,
        name_subdivision: None, form_subdivision: None,
        general_subdivision: None, chronological_subdivision: None,
        geographic_subdivision: None, source: None,
        authority_number: None, other_subfields: Vec::new(),
    }
}

fn author_to_personal(author: &Author) -> PersonalNameData {

    PersonalNameData {
        name_type: PersonalNameType::default(),
        name: author.lastname.clone().unwrap_or_default(),
        numeration: author.firstname.clone(),
        titles: None, dates: None,
        relator_term: None,
        fuller_form: None,
        relator_code: author.function.clone(),
        authority_number: None, dates_of_work: None,
        other_subfields: Vec::new(),
    }
}

/// Sync or insert a specific Note variant, preserving other notes.
fn sync_note<F>(notes: &mut Vec<Note>, matcher: F, new_note: Note)
where
    F: Fn(&Note) -> bool,
{
    if let Some(pos) = notes.iter().position(&matcher) {
        notes[pos] = new_note;
    } else {
        notes.push(new_note);
    }
}

/// Remove all notes matching a predicate.
fn remove_notes<F>(notes: &mut Vec<Note>, matcher: F)
where
    F: Fn(&Note) -> bool,
{
    notes.retain(|n| !matcher(n));
}

impl From<&Item> for MarcRecord {
    fn from(item: &Item) -> Self {
        use z3950_rs::marc_rs::fields::edition::Edition as MarcEdition;
        use z3950_rs::marc_rs::leader::*;

        // Start from existing marc_record if present, otherwise create a fresh record
        let mut record = item
            .marc_record
            .as_ref()
            .and_then(|v| serde_json::from_value::<MarcRecord>(v.clone()).ok())
            .unwrap_or_else(|| {
                MarcRecord::new(
                    Leader::builder()
                        .record_status(RecordStatus::New)
                        .record_type(RecordType::LanguageMaterial)
                        .bibliographic_level(BibliographicLevel::Monograph)
                        .character_coding_scheme(CharacterCodingScheme::Utf8)
                        .build(),
                )
            });

        // --- Title: update existing TitleStatement or insert as first ---
        if let Some(ref title) = item.title {
            if let Some(pos) = record.titles.iter().position(|t| matches!(t, Title::TitleStatement(_))) {
                if let Title::TitleStatement(ref existing) = record.titles[pos] {
                    let mut updated = existing.clone();
                    updated.title = title.clone();
                    record.titles[pos] = Title::TitleStatement(updated);
                }
            } else {
                record.titles.insert(0, Title::TitleStatement(TitleStatementData {
                    title_added_entry: true, nonfiling_chars: 0,
                    title: title.clone(),
                    remainder: None, responsibility: None,
                    other_title_info: None, first_responsibility: None,
                    other_responsibility: None, medium: None,
                    number_of_part: None, name_of_part: None,
                    other_subfields: Vec::new(),
                }));
            }
        }

        // --- ISBNs: rebuild from Item (replaces all) ---
        if let Some(ref isbn_str) = item.isbn {
            record.isbns.clear();
            for (i, part) in isbn_str.split(", ").enumerate() {
                record.isbns.push(Isbn {
                    number: part.to_string(),
                    qualification: None,
                    price_or_acquisition: if i == 0 { item.price.clone() } else { None },
                    cancelled_invalid: None,
                    other_subfields: Vec::new(),
                });
            }
        }

        // --- Authors: rebuild personal names, preserve corporate/meeting/uniform ---
        record.main_entries.retain(|e| !matches!(e, MainEntry::PersonalName(_)));
        record.added_entries.retain(|e| !matches!(e, AddedEntry::PersonalName(_)));

        for (idx, author) in item.authors.iter().enumerate() {
            let personal = author_to_personal(author);
            if idx == 0 {
                record.main_entries.insert(0, MainEntry::PersonalName(personal));
            } else {
                record.added_entries.push(AddedEntry::PersonalName(personal));
            }
            
        }

        // --- Publication: update first Publication entry or add one ---
        if let Some(ref edition) = item.edition {
            let pub_data = PublicationData {
                is_rda: false, function: None,
                places: edition.place_of_publication.clone().into_iter().collect(),
                publishers: edition.publisher_name.clone().into_iter().collect(),
                dates: edition.date.clone().into_iter().collect(),
                manufacturing_places: Vec::new(),
                manufacturing_dates: Vec::new(),
                other_subfields: Vec::new(),
            };
            if let Some(pos) = record.editions.iter().position(|e| matches!(e, MarcEdition::Publication(_))) {
                record.editions[pos] = MarcEdition::Publication(pub_data);
            } else {
                record.editions.push(MarcEdition::Publication(pub_data));
            }
        } else if let Some(ref pub_date) = item.publication_date {
            let pub_data = PublicationData {
                is_rda: false, function: None,
                places: Vec::new(), publishers: Vec::new(),
                dates: vec![pub_date.clone()],
                manufacturing_places: Vec::new(),
                manufacturing_dates: Vec::new(),
                other_subfields: Vec::new(),
            };
            if let Some(pos) = record.editions.iter().position(|e| matches!(e, MarcEdition::Publication(_))) {
                record.editions[pos] = MarcEdition::Publication(pub_data);
            } else {
                record.editions.push(MarcEdition::Publication(pub_data));
            }
        }

        // --- Physical description: update first or add ---
        if item.page_extent.is_some() || item.format.is_some() || item.accompanying_material.is_some() {
            let phys = PhysicalDescriptionData {
                extent: item.page_extent.clone().unwrap_or_default(),
                other_physical_details: None,
                dimensions: item.format.clone(),
                accompanying_material: item.accompanying_material.clone(),
                other_subfields: Vec::new(),
            };
            if let Some(pos) = record.physical.iter().position(|p| matches!(p, Physical::PhysicalDescription(_))) {
                record.physical[pos] = Physical::PhysicalDescription(phys);
            } else {
                record.physical.insert(0, Physical::PhysicalDescription(phys));
            }
        }

        // --- Series: update first SeriesStatement/SeriesTitle or add ---
        if let Some(ref series) = item.series {
            if let Some(ref name) = series.name {
                let sd = SeriesStatementData {
                    traced: false,
                    statement: name.clone(),
                    volume: item.series_volume_number.map(|v| v.to_string()),
                    issn: series.issn.clone(),
                    subseries: None,
                    other_subfields: Vec::new(),
                };
                if let Some(pos) = record.series.iter().position(|s| {
                    matches!(s, Series::SeriesStatement(_) | Series::SeriesTitle(_))
                }) {
                    record.series[pos] = Series::SeriesStatement(sd);
                } else {
                    record.series.push(Series::SeriesStatement(sd));
                }
            }
        }

        // --- Collection (Linking::MainSeriesEntry): update or add ---
        if let Some(ref collection) = item.collection {
            if let Some(ref coll_title) = collection.primary_title {
                let ld = LinkingData {
                    display_note: true,
                    title: Some(coll_title.clone()),
                    record_control_number: None,
                    issn: collection.issn.clone(),
                    isbn: None,
                    volume: item.collection_volume_number.map(|v| v.to_string()),
                    link_identifier: None,
                    other_subfields: Vec::new(),
                };
                if let Some(pos) = record.linking.iter().position(|l| matches!(l, Linking::MainSeriesEntry(_))) {
                    record.linking[pos] = Linking::MainSeriesEntry(ld);
                } else {
                    record.linking.push(Linking::MainSeriesEntry(ld));
                }
            }
        }

        // --- Notes: sync specific types, preserve the rest ---
        if let Some(ref notes) = item.notes {
            sync_note(&mut record.notes, |n| matches!(n, Note::GeneralNote(_)),
                Note::GeneralNote(make_note(notes.clone())));
        } else {
            remove_notes(&mut record.notes, |n| matches!(n, Note::GeneralNote(_)));
        }

        if let Some(ref content) = item.table_of_contents {
            sync_note(&mut record.notes, |n| matches!(n, Note::FormattedContentsNote(_)),
                Note::FormattedContentsNote(make_note(content.clone())));
        } else {
            remove_notes(&mut record.notes, |n| matches!(n, Note::FormattedContentsNote(_)));
        }

        if let Some(ref abstract_) = item.abstract_ {
            sync_note(&mut record.notes, |n| matches!(n, Note::Summary(_)),
                Note::Summary(make_note(abstract_.clone())));
        } else {
            remove_notes(&mut record.notes, |n| matches!(n, Note::Summary(_)));
        }

        // --- Subjects: rebuild TopicalTerm + IndexTermUncontrolled, preserve others ---
        record.subjects.retain(|s| !matches!(s,
            Subject::SubjectTopicalTerm(_) | Subject::IndexTermUncontrolled(_)
        ));

        if let Some(ref subject) = item.subject {
            record.subjects.insert(0, Subject::SubjectTopicalTerm(make_subject(subject.clone())));
        }

        if let Some(ref keywords) = item.keywords {
            for kw in keywords.split(", ") {
                if !kw.is_empty() {
                    record.subjects.push(Subject::IndexTermUncontrolled(make_subject(kw.to_string())));
                }
            }
        }

        // --- Dewey / Call number: replace all ---
        record.classifications.clear();
        if let Some(ref call_number) = item.call_number {
            record.classifications.push(DeweyClassification {
                is_additional: false,
                edition_type: DeweyEditionType::default(),
                assigned_by_lc: None,
                numbers: vec![call_number.clone()],
                item_number: None, edition: None,
                other_subfields: Vec::new(),
            });
        }

        // --- Languages: replace all ---
        record.languages.clear();
        if let Some(lang) = item.lang {
            if let Some(code) = language_id_to_code(lang) {
                let mut codes = vec![LanguageCode::from_code(code)];
                if let Some(lang_orig) = item.lang_orig {
                    if let Some(orig_code) = language_id_to_code(lang_orig) {
                        codes.push(LanguageCode::from_code(orig_code));
                    }
                }
                record.languages.push(LanguageData {
                    is_translation: if item.lang_orig.is_some() { Some(true) } else { None },
                    codes,
                    other_subfields: Vec::new(),
                });
            }
        }

        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_isbn() {
        assert_eq!(normalize_isbn("978-2-07-040850-4"), "9782070408504");
        assert_eq!(normalize_isbn("2 07 040850 4"), "2070408504");
    }

    #[test]
    fn test_parse_author_name() {
        let (last, first) = parse_author_name("Tolkien, J.R.R.");
        assert_eq!(last, "Tolkien");
        assert_eq!(first, Some("J.R.R.".to_string()));

        let (last, first) = parse_author_name("Anonymous");
        assert_eq!(last, "Anonymous");
        assert_eq!(first, None);
    }

    #[test]
    fn test_language_code() {
        assert_eq!(language_code_to_id("fre"), Some(1));
        assert_eq!(language_code_to_id("fra"), Some(1));
        assert_eq!(language_code_to_id("eng"), Some(2));
        assert_eq!(language_code_to_id("xxx"), Some(0));
    }

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
