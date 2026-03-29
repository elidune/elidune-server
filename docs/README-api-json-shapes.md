# API JSON Shapes — Frontend Reference

All JSON request bodies and response objects use **camelCase** keys.  
This document lists every public type with its exact JSON field names.

**Conventions:**
- IDs are serialized as **strings** (Snowflake i64 encoded as JSON string)
- Dates/times are **ISO 8601** strings (`"2026-03-24T10:00:00Z"`)
- Optional fields are omitted when null unless noted
- Status enums that are **single-word** stay lowercase (e.g. `"pending"`, `"open"`)

---

## Auth (`/api/v1/auth`)

### `POST /auth/login`

Request body — `LoginRequest`:
```json
{ "username": "jdoe", "password": "secret", "deviceId": "uuid-optional" }
```

Response — `LoginResponse`:
```json
{
  "token": "eyJ...",
  "tokenType": "Bearer",
  "expiresIn": 86400,
  "user": { ...UserInfo... },
  "requires2fa": false,
  "twoFactorMethod": null,
  "deviceId": null,
  "mustChangePassword": false
}
```

### `UserInfo` (embedded in login response)
```json
{
  "id": "927364819265437697",
  "login": "jdoe",
  "email": "jdoe@example.com",
  "firstname": "Jean",
  "lastname": "Doe",
  "addrStreet": "12 rue de la Paix",
  "addrZipCode": 75001,
  "addrCity": "Paris",
  "phone": "+33612345678",
  "birthdate": "1990-01-15",
  "accountType": "librarian",
  "language": "french"
}
```

### `POST /auth/verify-2fa`

Request — `Verify2FARequest`:
```json
{ "userId": "927364819265437697", "code": "123456", "deviceId": null, "trustDevice": false }
```

Response — `Verify2FAResponse`:
```json
{ "token": "eyJ...", "tokenType": "Bearer", "expiresIn": 86400 }
```

### `POST /auth/verify-recovery`

Request — `VerifyRecoveryRequest`:
```json
{ "userId": "927364819265437697", "code": "ABCD-EFGH-IJKL" }
```

### `POST /auth/request-password-reset`

Request — `RequestPasswordResetRequest`:
```json
{ "identifier": "jdoe@example.com", "resetUrl": "https://app.example.com/reset?token=<token>" }
```

### `POST /auth/reset-password`

Request — `ResetPasswordRequest`:
```json
{ "token": "reset-token-from-email", "newPassword": "newSecret123" }
```

### `POST /auth/setup-2fa`

Request — `Setup2FARequest`:
```json
{ "method": "totp" }
```

Response — `Setup2FAResponse`:
```json
{ "provisioningUri": "otpauth://...", "recoveryCodes": ["ABCD-1234", ...] }
```

### `POST /auth/change-password`

Request — `ChangePasswordRequest`:
```json
{ "newPassword": "newSecret123" }
```

---

## Users (`/api/v1/users`)

### `User` (full user object)
```json
{
  "id": "927364819265437697",
  "groupId": null,
  "barcode": "LIB-0001",
  "login": "jdoe",
  "firstname": "Jean",
  "lastname": "Doe",
  "email": "jdoe@example.com",
  "addrStreet": "12 rue de la Paix",
  "addrZipCode": 75001,
  "addrCity": "Paris",
  "phone": "+33612345678",
  "birthdate": "1990-01-15",
  "createdAt": "2026-01-01T10:00:00Z",
  "updateAt": "2026-03-01T09:00:00Z",
  "issueAt": null,
  "accountType": "librarian",
  "fee": "free",
  "publicType": "818273645564928001",
  "notes": null,
  "status": 0,
  "archivedAt": null,
  "language": "french",
  "sex": null,
  "staffType": null,
  "hoursPerWeek": null,
  "staffStartDate": null,
  "staffEndDate": null,
  "twoFactorEnabled": false,
  "twoFactorMethod": null,
  "receiveReminders": true,
  "mustChangePassword": false
}
```

### `UserShort` (embedded in loans, holds, etc.)
```json
{
  "id": "927364819265437697",
  "firstname": "Jean",
  "lastname": "Doe",
  "accountType": "librarian",
  "publicType": "818273645564928001",
  "nbLoans": 3,
  "nbLateLoans": 0
}
```

### `UserQuery` (GET /users query params)
| Param | camelCase key |
|-------|---------------|
| `name` | `name` |
| `barcode` | `barcode` |
| `page` | `page` |
| `perPage` | `perPage` |

### `UserPayload` (POST/PUT /users body)
```json
{
  "barcode": "LIB-0002",
  "login": "jsmith",
  "password": "secret",
  "firstname": "John",
  "lastname": "Smith",
  "email": "jsmith@example.com",
  "addrStreet": "5 avenue Victor Hugo",
  "addrZipCode": 69001,
  "addrCity": "Lyon",
  "phone": null,
  "birthdate": null,
  "accountType": "reader",
  "fee": "free",
  "publicType": "818273645564928001",
  "groupId": null,
  "sex": null,
  "staffType": null,
  "hoursPerWeek": null,
  "staffStartDate": null,
  "staffEndDate": null
}
```

### `UpdateProfile` (PATCH /auth/profile)
```json
{
  "firstname": "Jean-Marie",
  "email": "jm@example.com",
  "addrStreet": "...",
  "addrZipCode": 75001,
  "addrCity": "Paris",
  "currentPassword": "old",
  "newPassword": "new"
}
```

---

## Loans (`/api/v1/loans`)

### `CreateLoanRequest` (POST /loans body)
```json
{ "userId": "927364819265437697", "itemId": "818273645564928001", "itemIdentification": null, "force": false }
```

### `LoanResponse` (POST /loans response)
```json
{ "id": "927364819265437700", "issueAt": "2026-04-24T00:00:00Z", "message": "Loan created" }
```

### `LoanDetails` (GET /loans/:id, embedded in return response)
```json
{
  "id": "927364819265437700",
  "startDate": "2026-03-24T10:00:00Z",
  "issueAt": "2026-04-24T00:00:00Z",
  "renewalDate": null,
  "nbRenews": 0,
  "returnedAt": null,
  "biblio": { ...BiblioShort... },
  "user": { ...UserShort... },
  "itemIdentification": "978-2-07-040850-4",
  "isOverdue": false
}
```

### `ReturnResponse` (POST /loans/:id/return)
```json
{ "status": "returned", "loan": { ...LoanDetails... } }
```

### Query params — `OverdueLoansQuery`
`?page=1&perPage=20`

### Query params — `SendRemindersQuery`
`?dryRun=true`

### Query params — `GetUserLoansQuery`
`?archived=false`

---

## Biblios & Items

### `CreateBiblioQuery` / `CreateItemQuery` (query params)
`?allowDuplicateIsbn=false&confirmReplaceExistingId=123`

### `GetItemQuery` (query params for `GET /items/:id`)
`?fullRecord=false`

### `UpdateItemQuery` (query params for `PUT /items/:id`)
`?allowDuplicateIsbn=false`

### `ImportMarcBatchQuery` (query params for `POST /items/import-marc-batch`)
`?sourceId=100000000000000001&batchId=927364819265437696&recordId=1`

### `CreateBiblioResponse` / `CreateItemResponse`
```json
{ "biblio": { ...Biblio... }, "importReport": { ...ImportReport... } }
```

### `ImportReport`
```json
{
  "action": "mergedBibliographic",
  "existingId": "123456789",
  "warnings": ["No ISBN found"],
  "message": "Merged into existing record"
}
```

`action` values: `created` | `mergedBibliographic` | `replacedArchived` | `replacedConfirmed`

### `DuplicateConfirmationRequired` (409 body)
```json
{
  "code": "duplicate_isbn_needs_confirmation",
  "existingId": "123456789",
  "existingBiblio": { ...BiblioShort... },
  "message": "A biblio with the same ISBN already exists"
}
```

### `DuplicateItemBarcodeRequired` (409 body)
```json
{
  "code": "duplicate_barcode_needs_confirmation",
  "existingId": "987654321",
  "existingItem": { ...ItemShort... },
  "message": "An item with this barcode already exists"
}
```

### `Biblio` (full bibliographic record)
```json
{
  "id": "927364819265437697",
  "mediaType": "b",
  "isbn": "978-2-07-040850-4",
  "title": "Sherlock Holmes",
  "subject": null,
  "audienceType": "adult",
  "lang": "french",
  "langOrig": null,
  "publicationDate": "1995",
  "pageExtent": "250",
  "format": null,
  "tableOfContents": null,
  "accompanyingMaterial": null,
  "abstract": null,
  "notes": null,
  "keywords": ["detective", "mystery"],
  "isValid": 1,
  "seriesIds": [],
  "seriesVolumeNumbers": [],
  "editionId": null,
  "collectionIds": [],
  "collectionVolumeNumbers": [],
  "createdAt": "2026-01-01T10:00:00Z",
  "updatedAt": null,
  "archivedAt": null,
  "authors": [{ ...Author... }],
  "series": [],
  "collections": [],
  "edition": null,
  "items": []
}
```

`audienceType` values: `juvenile` | `preschool` | `primary` | `children` | `youngAdult` | `adultSerious` | `adult` | `general` | `specialized` | `unknown`

### `BiblioShort` (embedded in loans, tasks, etc.)
```json
{
  "id": "927364819265437697",
  "mediaType": "b",
  "isbn": "978-2-07-040850-4",
  "title": "Sherlock Holmes",
  "date": "1995",
  "status": 0,
  "isValid": 1,
  "archivedAt": null,
  "author": { ...Author... },
  "items": [{ ...ItemShort... }]
}
```

### `Author` (embedded in Biblio.authors)
```json
{
  "id": "819283746556492801",
  "key": null,
  "lastname": "Conan Doyle",
  "firstname": "Arthur",
  "bio": null,
  "notes": null,
  "function": "author"
}
```

`function` values: `author` | `illustrator` | `translator` | `scientificAdvisor` | `prefaceWriter` | `photographer` | `publishingDirector` | `composer`

### `BiblioAuthor` (junction row in /biblio-authors)
```json
{
  "id": "100000000000000010",
  "biblioId": "927364819265437697",
  "authorId": "819283746556492801",
  "role": null,
  "authorType": 0,
  "position": 0
}
```

### `Serie` (embedded in Biblio.series)
```json
{
  "id": "200000000000000001",
  "key": null,
  "name": "Les Aventures",
  "issn": null,
  "createdAt": "2026-01-01T10:00:00Z",
  "updatedAt": null,
  "volumeNumber": 3
}
```

### `Collection` (embedded in Biblio.collections)
```json
{
  "id": "200000000000000002",
  "key": null,
  "name": "Bibliothèque de la Pléiade",
  "secondaryTitle": null,
  "tertiaryTitle": null,
  "issn": null,
  "createdAt": "2026-01-01T10:00:00Z",
  "updatedAt": null,
  "volumeNumber": null
}
```

### `Edition` (embedded in Biblio.edition)
```json
{
  "id": "200000000000000003",
  "publisherName": "Gallimard",
  "placeOfPublication": "Paris",
  "date": "1995",
  "createdAt": "2026-01-01T10:00:00Z",
  "updatedAt": null
}
```

### `Item` (physical copy / specimen)
```json
{
  "id": "818273645564928001",
  "biblioId": "927364819265437697",
  "sourceId": "100000000000000001",
  "barcode": "978-2-07-040850-4",
  "callNumber": "FIC DOY",
  "volumeDesignation": null,
  "place": null,
  "borrowable": true,
  "circulationStatus": null,
  "notes": null,
  "price": null,
  "createdAt": "2026-01-01T10:00:00Z",
  "updatedAt": null,
  "archivedAt": null,
  "sourceName": "Fonds général"
}
```

### `ItemShort`
```json
{ "id": "818273645564928001", "barcode": "978-2-07-040850-4", "callNumber": "FIC DOY", "borrowable": true, "sourceName": "Fonds général" }
```

---

## Z39.50 (`/api/v1/z3950`)

### `Z3950SearchQuery` (query params)
`?query=doyle+sherlock&serverId=100000000000000002&maxResults=20`

### `Z3950SearchResponse`
```json
{ "total": 5, "biblios": [...], "source": "BnF Z39.50" }
```

### `Z3950ImportRequest`
```json
{
  "biblioId": "818273645564928001",
  "items": [{ "barcode": "978-2-07-040850-4", "callNumber": "FIC DOY", "sourceId": "100000000000000001" }],
  "confirmReplaceExistingId": null
}
```

### `ImportItem` (nested in Z3950ImportRequest)
```json
{ "barcode": "978-2-07-040850-4", "callNumber": "FIC DOY", "status": null, "place": null, "notes": null, "price": null, "sourceId": "100000000000000001" }
```

### `Z3950ImportResponse`
```json
{ "biblio": { ...Biblio... }, "importReport": { ...ImportReport... } }
```

---

## Sources (`/api/v1/sources`)

### `SourcesQuery` (query params)
`?includeArchived=false`

### `Source`
```json
{ "id": "100000000000000001", "key": "fonds-general", "name": "Fonds général", "isArchive": null, "archivedAt": null, "default": true }
```

### `MergeSources`
```json
{ "sourceIds": ["100000000000000001", "100000000000000002"], "name": "Fonds unifié" }
```

---

## Public Types (`/api/v1/public-types`)

### `PublicType`
```json
{
  "id": "818273645564928001",
  "name": "adult",
  "label": "Adulte",
  "subscriptionDurationDays": 365,
  "ageMin": 18,
  "ageMax": null,
  "subscriptionPrice": 1500,
  "maxLoans": 5,
  "loanDurationDays": 28
}
```

### `PublicTypeLoanSettings`
```json
{ "id": "...", "publicTypeId": "...", "mediaType": "b", "duration": 14, "nbMax": 3, "nbRenews": 1 }
```

### `UpsertLoanSettingRequest`
```json
{ "mediaType": "b", "duration": 14, "nbMax": 3, "nbRenews": 1 }
```

### `CreatePublicType` / `UpdatePublicType`
```json
{
  "name": "youth",
  "label": "Jeunesse",
  "subscriptionDurationDays": 365,
  "ageMin": null,
  "ageMax": 18,
  "subscriptionPrice": 500,
  "maxLoans": 5,
  "loanDurationDays": 14
}
```

---

## Holds (`/api/v1/holds`)

List endpoints (`GET /holds`, `GET /items/:id/holds`, `GET /users/:id/holds`) return **`HoldDetails`**. Create/cancel responses use plain **`Hold`** (ids only, no embedded item/user).

### `Hold`
```json
{
  "id": "927364819265437701",
  "userId": "927364819265437697",
  "itemId": "818273645564928001",
  "createdAt": "2026-03-24T10:00:00Z",
  "notifiedAt": null,
  "expiresAt": null,
  "status": "pending",
  "position": 1,
  "notes": null
}
```

`status` values: `pending` | `ready` | `fulfilled` | `cancelled` | `expired`

### `HoldDetails`
```json
{
  "id": "927364819265437701",
  "biblio": { ...BiblioShort... },
  "user": { ...UserShort... },
  "createdAt": "2026-03-24T10:00:00Z",
  "notifiedAt": null,
  "expiresAt": null,
  "status": "pending",
  "position": 1,
  "notes": null
}
```

`biblio.items` has exactly **one** `ItemShort` (the copy this hold is on).

### `CreateHold`
```json
{ "userId": "927364819265437697", "itemId": "818273645564928001", "notes": null }
```

---

## Fines (`/api/v1/fines`)

### `Fine`
```json
{
  "id": "927364819265437702",
  "loanId": "927364819265437700",
  "userId": "927364819265437697",
  "amount": "3.50",
  "paidAmount": "0.00",
  "createdAt": "2026-03-24T10:00:00Z",
  "paidAt": null,
  "status": "pending",
  "notes": null
}
```

`status` values: `pending` | `partial` | `paid` | `waived`

### `FineRule`
```json
{ "id": 1, "mediaType": "b", "dailyRate": "0.10", "maxAmount": "5.00", "graceDays": 3, "notes": null }
```

### `UpsertFineRuleRequest` (PUT /fines/rules body)
```json
{ "mediaType": "b", "dailyRate": "0.10", "maxAmount": "5.00", "graceDays": 3 }
```

### `UnpaidFinesSummary` (GET /users/:id/fines response)
```json
{ "totalUnpaid": "3.50", "fines": [...Fine...] }
```

---

## Inventory (`/api/v1/inventory`)

See **[Inventory API — frontend guide](inventory-api-frontend.md)** for semantics, query params, and error codes.

| Method | Path | Response |
|--------|------|----------|
| `GET` | `/inventory/sessions` | `PaginatedResponse<InventorySession>` (`page`, `perPage`, …) |
| `POST` | `/inventory/sessions` | `InventorySession` (**201**) |
| `GET` | `/inventory/sessions/:id` | `InventorySession` |
| `POST` | `/inventory/sessions/:id/close` | `InventorySession` |
| `POST` | `/inventory/sessions/:id/scan` | `InventoryScan` (**201**) |
| `POST` | `/inventory/sessions/:id/scans/batch` | `InventoryScan[]` (**201**), max **500** barcodes |
| `GET` | `/inventory/sessions/:id/scans` | `PaginatedResponse<InventoryScan>` |
| `GET` | `/inventory/sessions/:id/missing` | `PaginatedResponse<InventoryMissingRow>` |
| `GET` | `/inventory/sessions/:id/report` | `InventoryReport` |

### `InventorySession`
```json
{
  "id": "927364819265437703",
  "name": "Inventaire printemps 2026",
  "startedAt": "2026-03-24T08:00:00Z",
  "closedAt": null,
  "status": "open",
  "locationFilter": "Salle A",
  "notes": null,
  "scopePlace": 3,
  "createdBy": "927364819265437697"
}
```

`status` values: `open` | `closed`. `scopePlace` is **`null`** when the session covers all active items.

### `CreateInventorySession` (`POST /inventory/sessions`)
```json
{ "name": "Inventaire printemps 2026", "locationFilter": "Salle A", "notes": null, "scopePlace": 3 }
```

### `ScanBarcode` (`POST /inventory/sessions/:id/scan`)
```json
{ "barcode": "978-2-07-040850-4" }
```

### `BatchScanBarcodes` (`POST /inventory/sessions/:id/scans/batch`)
```json
{ "barcodes": ["978-2-07-040850-4", "9782123456789"] }
```

### `InventoryScan`
```json
{
  "id": 42,
  "sessionId": "927364819265437703",
  "itemId": "818273645564928001",
  "barcode": "978-2-07-040850-4",
  "scannedAt": "2026-03-24T09:00:00Z",
  "result": "found",
  "scannedBy": "927364819265437697"
}
```

`id` is a JSON **number**. `itemId` is **null** only for `unknown_barcode`.

`result` values: `found` | `found_archived` | `unknown_barcode`

### `InventoryMissingRow`
```json
{
  "itemId": "818273645564928002",
  "barcode": "9782123456789",
  "callNumber": "PQ 1234",
  "place": 3,
  "biblioTitle": "Example title"
}
```

### `InventoryReport`
```json
{
  "sessionId": "927364819265437703",
  "expectedInScope": 1200,
  "totalScanned": 320,
  "totalFound": 300,
  "totalFoundArchived": 5,
  "totalUnknown": 15,
  "distinctItemsScanned": 290,
  "duplicateScanCount": 15,
  "missingCount": 880,
  "missingScannable": 870,
  "missingWithoutBarcode": 10
}
```

`sessionId` is a JSON **string** (Snowflake), consistent with other inventory ids.

---

## Events (`/api/v1/events`)

### `Event`
```json
{
  "id": "927364819265437704",
  "name": "Heure du conte",
  "eventType": 0,
  "eventDate": "2026-04-10",
  "startTime": "10:00",
  "endTime": "11:00",
  "attendeesCount": 15,
  "targetPublic": 106,
  "schoolName": null,
  "className": null,
  "studentsCount": null,
  "partnerName": null,
  "description": null,
  "notes": null,
  "createdAt": "2026-03-01T10:00:00Z",
  "updateAt": null,
  "announcementSentAt": null
}
```

### `EventsListResponse`
```json
{ "events": [...Event...], "total": 42 }
```

### `EventQuery` (query params)
`?startDate=2026-01-01&endDate=2026-12-31&eventType=0&page=1&perPage=20`

### `EventAnnualStats`
```json
{
  "totalEvents": 48,
  "totalAttendees": 520,
  "schoolVisits": 12,
  "distinctClasses": 8,
  "totalStudents": 240,
  "byType": [{ "eventType": 0, "count": 24, "attendees": 360 }]
}
```

---

## Schedules (`/api/v1/schedules`)

### `SchedulePeriod`
```json
{ "id": "...", "name": "Horaires hiver 2026", "startDate": "2026-01-01", "endDate": "2026-03-31", "notes": null, "createdAt": "...", "updateAt": null }
```

### `ScheduleSlot`
```json
{ "id": "...", "periodId": "...", "dayOfWeek": 1, "openTime": "09:00", "closeTime": "18:00", "createdAt": "..." }
```

### `ScheduleClosure`
```json
{ "id": "...", "closureDate": "2026-01-01", "reason": "Jour férié", "createdAt": "..." }
```

### `ScheduleClosureQuery` (query params)
`?startDate=2026-01-01&endDate=2026-12-31`

---

## Visitor Counts (`/api/v1/visitor-counts`)

### `VisitorCount`
```json
{ "id": "...", "countDate": "2026-03-24", "count": 87, "source": "manual", "notes": null, "createdAt": "..." }
```

### `VisitorCountQuery` (query params)
`?startDate=2026-01-01&endDate=2026-12-31`

---

## Equipment (`/api/v1/equipment`)

### `Equipment`
```json
{
  "id": "...",
  "name": "PC Multimédia 1",
  "equipmentType": 0,
  "hasInternet": true,
  "isPublic": true,
  "quantity": 1,
  "status": 0,
  "notes": null,
  "createdAt": "...",
  "updateAt": null
}
```

---

## Settings (`/api/v1/settings`)

### `SettingsResponse`
```json
{
  "loanSettings": [
    { "mediaType": "b", "maxLoans": 5, "maxRenewals": 2, "durationDays": 28 }
  ],
  "z3950Servers": [
    { "id": "...", "name": "BnF", "address": "z3950.bnf.fr", "port": 2211, "database": "TOUT", "format": "UNIMARC", "login": null, "password": null, "encoding": "utf-8", "isActive": true }
  ]
}
```

---

## Library Info (`/api/v1/library-info`)

### `LibraryInfo`
```json
{
  "name": "Médiathèque Municipale",
  "addrLine1": "1 place de l'Hôtel de Ville",
  "addrLine2": null,
  "addrPostcode": "75001",
  "addrCity": "Paris",
  "addrCountry": "France",
  "phones": ["+33 1 23 45 67 89"],
  "email": "mediatheque@mairie.fr",
  "updatedAt": "2026-01-15T10:00:00Z"
}
```

---

## Admin Config (`/api/v1/admin/config`)

### `ConfigResponse`
```json
{
  "sections": [
    { "key": "email", "value": { ...json... }, "overridden": false, "overridable": true }
  ]
}
```

### `ReindexSearchResponse`
```json
{ "itemsQueued": 1250, "meilisearchAvailable": true }
```

---

## Statistics (`/api/v1/stats`)

### `StatsQuery` (query params)
`?year=2026&startDate=2026-01-01&endDate=2026-12-31&publicType=adult&mediaType=b`

### `StatsResponse`
```json
{
  "items": {
    "total": 8420,
    "byMediaType": [{ "label": "b", "value": 7200 }],
    "byPublicType": [{ "label": "adult", "value": 5000 }],
    "acquisitions": 320,
    "acquisitionsByMediaType": [],
    "withdrawals": 45,
    "withdrawalsByMediaType": []
  },
  "users": {
    "total": 1250,
    "active": 480,
    "byAccountType": [{ "label": "reader", "value": 1100 }]
  },
  "loans": {
    "active": 210,
    "overdue": 18,
    "returnedToday": 5,
    "byMediaType": [{ "label": "b", "value": 180 }]
  }
}
```

### `LoanStatsQuery` (query params — `GET /stats/loans`)
`?startDate=2026-01-01&endDate=2026-12-31&interval=month&mediaType=b&publicType=adult&userId=927364819265437697`

`interval` values: `day` | `week` | `month` | `year`

### `LoanStatsResponse`
```json
{
  "totalLoans": 3820,
  "totalReturns": 3750,
  "timeSeries": [{ "period": "2026-01", "loans": 320, "returns": 310 }],
  "byMediaType": [{ "label": "b", "value": 3100 }]
}
```

### `UserStatsQuery` (query params — `GET /stats/users`)
`?sortBy=totalLoans&limit=50&startDate=2026-01-01&endDate=2026-12-31&mode=leaderboard`

`sortBy` values: `totalLoans` | `activeLoans` | `overdueLoans`  
`mode` values: `leaderboard` | `aggregate`

### `UserStatsResponse` — leaderboard mode
```json
{
  "mode": "leaderboard",
  "users": [
    { "userId": "...", "firstname": "Jean", "lastname": "Doe", "totalLoans": 42, "activeLoans": 3, "overdueLoans": 0 }
  ]
}
```

### `UserStatsResponse` — aggregate mode
```json
{
  "mode": "aggregate",
  "usersTotal": 1250,
  "usersByPublicType": [{ "label": "adult", "value": 900 }],
  "usersBySex": [],
  "newUsersTotal": 85,
  "newUsersByPublicType": [],
  "newUsersBySex": [],
  "activeBorrowersTotal": 480,
  "activeBorrowersByPublicType": [],
  "groupsTotal": 12
}
```

### `CatalogStatsQuery` (query params — `GET /stats/catalog`)
`?startDate=2026-01-01&endDate=2026-12-31&bySource=true&byMediaType=true&byPublicType=false`

### `CatalogStatsResponse`
```json
{
  "totals": { "activeItems": 8420, "enteredItems": 320, "archivedItems": 45, "loans": 3820 },
  "bySource": [
    { "sourceId": "...", "sourceName": "Fonds général", "activeItems": 7200, "enteredItems": 280, "archivedItems": 40, "loans": 3200, "byMediaType": null, "byPublicType": null }
  ],
  "byMediaType": null,
  "byPublicType": null
}
```

---

## Maintenance (`/api/v1/maintenance`)

### `MaintenanceRequest`
```json
{ "actions": ["cleanupSeries", "mergeDuplicateSeries", "cleanupOrphanAuthors"] }
```

Available `action` values:
| Value | Description |
|-------|-------------|
| `cleanupSeries` | Strip quotes from series names, delete orphan series |
| `cleanupCollections` | Strip quotes from collection names, delete orphan collections |
| `cleanupOrphanAuthors` | Delete authors not linked to any biblio |
| `mergeDuplicateSeries` | Merge series with identical names (case-insensitive) |
| `mergeDuplicateCollections` | Merge collections with identical names |
| `cleanupDanglingBiblioSeries` | Remove broken biblio↔series links |
| `cleanupDanglingBiblioCollections` | Remove broken biblio↔collection links |

Returns `202 Accepted` — poll `GET /tasks/:id` (see background tasks doc).

### `MaintenanceResponse` (task `result` when completed)
```json
{
  "reports": [
    { "action": "cleanupSeries", "success": true, "details": { "deleted": 12, "quotesStripped": 8 }, "error": null },
    { "action": "mergeDuplicateSeries", "success": false, "details": {}, "error": "timeout" }
  ]
}
```

---

## Background Tasks (`/api/v1/tasks`)

See [README-background-tasks.md](README-background-tasks.md) for full polling guide.

### `TaskAcceptedResponse` (202 on async endpoints)
```json
{ "taskId": "927364819265437705" }
```

### `BackgroundTask` (GET /tasks/:id)
```json
{
  "id": "927364819265437705",
  "kind": "marcBatchImport",
  "status": "running",
  "progress": { "current": 42, "total": 150, "message": "Importing record 42/150" },
  "result": null,
  "error": null,
  "createdAt": "2026-03-24T10:00:00Z",
  "startedAt": "2026-03-24T10:00:01Z",
  "completedAt": null,
  "userId": "927364819265437697"
}
```

`kind` values: `marcBatchImport` | `maintenance`  
`status` values: `pending` | `running` | `completed` | `failed`

### `MarcBatchImportReport` (task `result` when kind=`marcBatchImport`)
```json
{
  "batchId": "927364819265437696",
  "imported": ["1", "2", "5"],
  "failed": [{ "key": "3", "error": "Duplicate ISBN", "existingId": "42" }]
}
```

### MARC batch info — `MarcBatchInfo`
```json
{ "batchId": "927364819265437696", "recordCount": 150, "ttlSeconds": 3600 }
```

---

## Batch Operations (`/api/v1/loans/batch-*`)

### `BatchReturnRequest` (POST /loans/batch-return)
```json
{ "barcodes": ["978-2-07-040850-4", "978-2-07-040851-1"] }
```

### `BatchReturnResponse`
```json
{
  "returned": 2,
  "errors": 0,
  "results": [
    { "barcode": "978-2-07-040850-4", "loan": { ...LoanDetails... }, "success": true },
    { "barcode": "978-2-07-040851-1", "success": false, "error": "No active loan for this barcode" }
  ]
}
```

### `BatchCreateLoansRequest` (POST /loans/batch-create)
```json
{ "userId": "927364819265437697", "barcodes": ["978-2-07-040850-4"], "force": false }
```

### `BatchCreateLoansResponse`
```json
{
  "created": 1,
  "errors": 0,
  "results": [
    { "barcode": "978-2-07-040850-4", "success": true, "loanId": "927364819265437700" }
  ]
}
```

---

## History (`/api/v1/users/:id/history`)

### `HistoryPreference`
```json
{ "userId": "927364819265437697", "historyEnabled": true }
```

### `UpdateHistoryPreference` (PUT /users/:id/history/preference)
```json
{ "enabled": false }
```

---

## Audit Log (`/api/v1/audit`)

### `AuditLogEntry`
```json
{
  "id": 1234,
  "eventType": "loan.created",
  "userId": "927364819265437697",
  "entityType": "loan",
  "entityId": "927364819265437700",
  "ipAddress": "192.168.1.1",
  "payload": { "itemId": "818273645564928001" },
  "createdAt": "2026-03-24T10:00:00Z"
}
```

### `AuditQueryRequest` (query params for GET /audit)
`?eventType=loan.created&entityType=loan&entityId=123&userId=456&fromDate=2026-01-01T00:00:00Z&toDate=2026-12-31T23:59:59Z&page=1&perPage=50`

### `AuditExportRequest` (query params for GET /audit/export)
`?format=csv&eventType=loan.created&fromDate=2026-01-01T00:00:00Z&toDate=2026-12-31T23:59:59Z`

### `AuditLogPage`
```json
{ "entries": [...AuditLogEntry...], "total": 420, "page": 1, "perPage": 50 }
```

---

## Reminders

### `ReminderReport` (response to POST /loans/reminders)
```json
{
  "dryRun": false,
  "emailsSent": 18,
  "loansReminded": 18,
  "details": [
    { "userId": "...", "email": "jdoe@example.com", "firstname": "Jean", "lastname": "Doe", "loanCount": 2 }
  ],
  "errors": [
    { "userId": "...", "email": "noreply@example.com", "errorMessage": "Invalid email address" }
  ]
}
```

### `OverdueLoanInfo` (entries in GET /loans/overdue)
```json
{
  "loanId": "927364819265437700",
  "userId": "927364819265437697",
  "firstname": "Jean",
  "lastname": "Doe",
  "userEmail": "jdoe@example.com",
  "biblioId": "818273645564928000",
  "title": "Les Misérables",
  "authors": "Victor Hugo",
  "itemBarcode": "978-2-07-123456-7",
  "loanDate": "2026-02-01T10:00:00Z",
  "issueAt": "2026-03-01T10:00:00Z",
  "lastReminderSentAt": null,
  "reminderCount": 0
}
```

### `OverdueLoansPage`
```json
{ "loans": [...OverdueLoanInfo...], "total": 18, "page": 1, "perPage": 20 }
```

---

## Error Responses

All error responses share the same envelope:
```json
{ "code": "not_found", "error": "Not Found", "message": "Biblio 123 not found" }
```

Common `code` values:
| code | HTTP status |
|------|------------|
| `authentication` | 401 |
| `authorization` | 403 |
| `not_found` | 404 |
| `validation_error` | 400 |
| `conflict` | 409 |
| `business_rule` | 422 |
| `internal_error` | 500 |

---

## TypeScript Type Definitions

```typescript
// ── Core ID type ──────────────────────────────────────────────
type ID = string; // Snowflake i64 serialized as string

// ── Enums ─────────────────────────────────────────────────────
type Language     = 'french' | 'english' | 'german' | 'spanish' | 'portuguese' | 'japanese';
type AccountType  = 'admin' | 'librarian' | 'reader' | 'guest';
type FeeType      = 'free' | 'local' | 'foreigner';
type FineStatus   = 'pending' | 'partial' | 'paid' | 'waived';
type HoldStatus = 'pending' | 'ready' | 'fulfilled' | 'cancelled' | 'expired';
type InventoryStatus   = 'open' | 'closed';
type TaskKind     = 'marcBatchImport' | 'maintenance';
type TaskStatus   = 'pending' | 'running' | 'completed' | 'failed';
type ImportAction = 'created' | 'mergedBibliographic' | 'replacedArchived' | 'replacedConfirmed';
type Interval     = 'day' | 'week' | 'month' | 'year';
type UserStatsMode   = 'leaderboard' | 'aggregate';
type UserStatsSortBy = 'totalLoans' | 'activeLoans' | 'overdueLoans';
type MaintenanceAction =
  | 'cleanupSeries' | 'cleanupCollections' | 'cleanupOrphanAuthors'
  | 'mergeDuplicateSeries' | 'mergeDuplicateCollections'
  | 'cleanupDanglingBiblioSeries' | 'cleanupDanglingBiblioCollections';
type AuthorFunction =
  | 'author' | 'illustrator' | 'translator' | 'scientificAdvisor'
  | 'prefaceWriter' | 'photographer' | 'publishingDirector' | 'composer';
type AudienceType =
  | 'juvenile' | 'preschool' | 'primary' | 'children' | 'youngAdult'
  | 'adultSerious' | 'adult' | 'general' | 'specialized' | 'unknown';

// ── Users ─────────────────────────────────────────────────────
interface UserShort {
  id: ID; firstname: string | null; lastname: string | null;
  accountType: AccountType | null; publicType: ID | null;
  nbLoans: number | null; nbLateLoans: number | null;
}

// ── Biblios ───────────────────────────────────────────────────
interface Author {
  id: ID; key: string | null; lastname: string | null; firstname: string | null;
  bio: string | null; notes: string | null; function: AuthorFunction | null;
}
interface BiblioAuthor {
  id: ID; biblioId: ID; authorId: ID;
  role: string | null; authorType: number; position: number;
}
interface Serie {
  id: ID | null; key: string | null; name: string | null; issn: string | null;
  createdAt: string | null; updatedAt: string | null; volumeNumber: number | null;
}
interface Collection {
  id: ID | null; key: string | null; name: string | null;
  secondaryTitle: string | null; tertiaryTitle: string | null; issn: string | null;
  createdAt: string | null; updatedAt: string | null; volumeNumber: number | null;
}
interface Edition {
  id: ID | null; publisherName: string | null; placeOfPublication: string | null;
  date: string | null; createdAt: string | null; updatedAt: string | null;
}
interface ItemShort {
  id: ID; barcode: string | null; callNumber: string | null;
  borrowable: boolean; sourceName: string | null;
}
interface BiblioShort {
  id: ID; mediaType: string; isbn: string | null; title: string | null;
  date: string | null; status: number; isValid: number | null;
  archivedAt: string | null; author: Author | null; items: ItemShort[];
}
interface Biblio {
  id: ID | null; mediaType: string; isbn: string | null; title: string | null;
  subject: string | null; audienceType: AudienceType | null;
  lang: string | null; langOrig: string | null;
  publicationDate: string | null; pageExtent: string | null;
  format: string | null; tableOfContents: string | null;
  accompanyingMaterial: string | null; abstract: string | null;
  notes: string | null; keywords: string[] | null; isValid: number | null;
  seriesIds: ID[]; seriesVolumeNumbers: (number | null)[];
  editionId: ID | null;
  collectionIds: ID[]; collectionVolumeNumbers: (number | null)[];
  createdAt: string | null; updatedAt: string | null; archivedAt: string | null;
  authors: Author[]; series: Serie[]; collections: Collection[];
  edition: Edition | null; items: Item[];
}

// ── Loans ─────────────────────────────────────────────────────
interface LoanDetails {
  id: ID; startDate: string; issueAt: string;
  renewalDate: string | null; nbRenews: number;
  returnedAt: string | null; biblio: BiblioShort;
  user: UserShort | null; itemIdentification: string | null;
  isOverdue: boolean;
}

// ── Fines ─────────────────────────────────────────────────────
interface Fine {
  id: ID; loanId: ID; userId: ID;
  amount: string; paidAmount: string;
  createdAt: string; paidAt: string | null;
  status: FineStatus; notes: string | null;
}
interface FineRule {
  id: number; mediaType: string | null; dailyRate: string;
  maxAmount: string | null; graceDays: number; notes: string | null;
}
interface UnpaidFinesSummary { totalUnpaid: string; fines: Fine[]; }

// ── Holds ─────────────────────────────────────────────────────
interface Hold {
  id: ID; userId: ID; itemId: ID;
  createdAt: string; notifiedAt: string | null; expiresAt: string | null;
  status: HoldStatus; position: number; notes: string | null;
}
interface HoldDetails {
  id: ID;
  biblio: BiblioShort;
  user: UserShort | null;
  createdAt: string; notifiedAt: string | null; expiresAt: string | null;
  status: HoldStatus; position: number; notes: string | null;
}

// ── Batch ─────────────────────────────────────────────────────
interface BatchReturnItemResult {
  barcode: string; loan?: LoanDetails | null; success: boolean; error?: string;
}
interface BatchReturnResponse { returned: number; errors: number; results: BatchReturnItemResult[]; }
interface BatchCreateLoanItemResult {
  barcode: string; success: boolean; loanId?: string | null; error?: string;
}
interface BatchCreateLoansResponse { created: number; errors: number; results: BatchCreateLoanItemResult[]; }

// ── History ───────────────────────────────────────────────────
interface HistoryPreference { userId: ID; historyEnabled: boolean; }

// ── Tasks ─────────────────────────────────────────────────────
interface TaskProgress {
  current: number; total: number;
  message?: unknown; // may be string or structured object
}
interface BackgroundTask {
  id: ID; kind: TaskKind; status: TaskStatus;
  progress?: TaskProgress | null;
  result?: MarcBatchImportReport | MaintenanceResponse | null;
  error?: string | null;
  createdAt: string; startedAt?: string | null; completedAt?: string | null;
  userId: ID;
}
interface MarcBatchImportReport {
  batchId: ID; imported: string[];
  failed: Array<{ key: string; error: string; existingId?: string }>;
}
interface MaintenanceResponse {
  reports: Array<{ action: string; success: boolean; details: Record<string, number>; error?: string }>;
}

// ── Import Report ─────────────────────────────────────────────
interface ImportReport {
  action: ImportAction;
  existingId?: string;
  warnings: string[];
  message?: string;
}

// ── Item (physical copy) ──────────────────────────────────────
interface Item {
  id: ID; biblioId: ID; sourceId: ID | null; barcode: string | null;
  callNumber: string | null; volumeDesignation: string | null; place: string | null;
  borrowable: boolean; circulationStatus: string | null; notes: string | null;
  price: string | null; createdAt: string; updatedAt: string | null;
  archivedAt: string | null; sourceName: string | null;
}
```
