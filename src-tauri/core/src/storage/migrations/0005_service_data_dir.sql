-- Database and cache services keep their data in a folder of their own.
-- NULL for web servers. Zenv never deletes this folder.
ALTER TABLE services ADD COLUMN data_dir TEXT;
