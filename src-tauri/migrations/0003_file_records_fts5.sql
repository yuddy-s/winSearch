CREATE VIRTUAL TABLE IF NOT EXISTS file_records_fts USING fts5(
  file_id UNINDEXED,
  content_text,
  tokenize = "unicode61 remove_diacritics 2"
);

CREATE TRIGGER IF NOT EXISTS trg_file_records_ai_fts
AFTER INSERT ON file_records
WHEN NEW.content_indexed = 1
  AND NEW.content_text IS NOT NULL
  AND length(trim(NEW.content_text)) > 0
BEGIN
  INSERT INTO file_records_fts (file_id, content_text)
  VALUES (NEW.id, NEW.content_text);
END;

CREATE TRIGGER IF NOT EXISTS trg_file_records_au_fts
AFTER UPDATE OF id, content_text, content_indexed ON file_records
BEGIN
  DELETE FROM file_records_fts WHERE file_id = OLD.id;
  INSERT INTO file_records_fts (file_id, content_text)
  SELECT NEW.id, NEW.content_text
  WHERE NEW.content_indexed = 1
    AND NEW.content_text IS NOT NULL
    AND length(trim(NEW.content_text)) > 0;
END;

CREATE TRIGGER IF NOT EXISTS trg_file_records_ad_fts
AFTER DELETE ON file_records
BEGIN
  DELETE FROM file_records_fts WHERE file_id = OLD.id;
END;

INSERT INTO file_records_fts (file_id, content_text)
SELECT fr.id, fr.content_text
FROM file_records fr
WHERE fr.content_indexed = 1
  AND fr.content_text IS NOT NULL
  AND length(trim(fr.content_text)) > 0
  AND NOT EXISTS (
    SELECT 1
    FROM file_records_fts fts
    WHERE fts.file_id = fr.id
  );
