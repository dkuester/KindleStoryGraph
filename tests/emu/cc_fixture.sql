-- Fixture for the virtual Kindle (tests/emu): a minimal cc.db with only the
-- Entries columns KindleStoryGraph reads (see src/reader.rs). The emulator
-- builds /var/local/cc.db from this file on every run.
--
-- With min_progress=2.0 the snapshot holds 7 books (the expectations in
-- tests/emu/emu.sh depend on this — update both together).
CREATE TABLE Entries (
    p_uuid TEXT PRIMARY KEY,
    p_type TEXT,
    p_isArchived INTEGER,
    p_cdeKey TEXT,
    p_cdeType TEXT,
    p_titles_0_nominal TEXT,
    j_credits TEXT,
    p_percentFinished REAL,
    p_lastAccess INTEGER
);

INSERT INTO Entries VALUES
    -- In progress (the "current" book: most recent access).
    ('u-dune', 'Entry:Item', 0, 'B00DUNE001', 'EBOK', 'Dune',
     '[{"name":{"display":"Frank Herbert","collation":"Herbert, Frank"},"kind":"Author"}]',
     42.3, 1761700000),
    -- Archived cloud-only duplicate of Dune: no progress, must be ignored.
    ('u-dune-cloud', 'Entry:Item', 1, 'B00DUNE001', 'EBOK', 'Dune',
     '[{"name":{"display":"Frank Herbert","collation":"Herbert, Frank"},"kind":"Author"}]',
     NULL, 1761600000),
    -- Finished.
    ('u-phm', 'Entry:Item', 0, 'B08FHBV4ZX', 'EBOK', 'Project Hail Mary',
     '[{"name":{"display":"Andy Weir","collation":"Weir, Andy"},"kind":"Author"}]',
     100.0, 1761500000),
    -- Finished via the >= 99.5 rule (cc.db stores a rounded-down value).
    ('u-martian', 'Entry:Item', 0, 'B00EMXBDMA', 'EBOK', 'The Martian',
     '[{"name":{"display":"Andy Weir","collation":"Weir, Andy"},"kind":"Author"}]',
     99.6, 1761400000),
    -- Same title, different authors (two distinct books).
    ('u-outsider-king', 'Entry:Item', 0, 'B07D7CWN1L', 'EBOK', 'The Outsider',
     '[{"name":{"display":"Stephen King","collation":"King, Stephen"},"kind":"Author"}]',
     30.0, 1761300000),
    ('u-outsider-camus', 'Entry:Item', 0, 'B0058ZGGFE', 'EBOK', 'The Outsider',
     '[{"name":{"display":"Albert Camus","collation":"Camus, Albert"},"kind":"Author"}]',
     55.0, 1761200000),
    -- Non-ASCII title and author (UTF-8 through the menu on the device).
    ('u-schwarm', 'Entry:Item', 0, 'B004OL2OSI', 'EBOK', 'Der Schwarm',
     '[{"name":{"display":"Frank Schätzing","collation":"Schätzing, Frank"},"kind":"Author"}]',
     12.5, 1761100000),
    -- Personal document (sideloaded / Send-to-Kindle) with progress.
    ('u-pdoc', 'Entry:Item', 0, 'PDOC7XKQ2M9VBN4T', 'PDOC', 'Meeting notes 2026',
     '[{"name":{"display":"Daniel","collation":"Daniel"},"kind":"Author"}]',
     15.0, 1761000000),
    -- Peeked at only: below min_progress (Kindle jumps to ~1 % on first open).
    ('u-peek', 'Entry:Item', 0, 'B0PEEK0001', 'EBOK', 'Barely Opened',
     '[{"name":{"display":"Some Author","collation":"Author, Some"},"kind":"Author"}]',
     1.0, 1760900000),
    -- Never opened.
    ('u-unread', 'Entry:Item', 0, 'B0UNREAD01', 'EBOK', 'Still On The Pile',
     '[{"name":{"display":"Some Author","collation":"Author, Some"},"kind":"Author"}]',
     0.0, 1760800000),
    -- Jailbreak dummy document as it appears in the library.
    ('u-jb', 'Entry:Item', 0, 'JAILBROKEN0000000000', 'PDOC', 'You are Jailbroken',
     '[]', 0.0, 1760700000),
    -- Not a book at all (collection row): filtered by p_type.
    ('u-coll', 'Entry:Collection', 0, 'COLL-SCIFI', NULL, 'Sci-Fi',
     NULL, 80.0, 1761800000);
