-- Languages are always available; each site picks its version. Only servers
-- keep an on/off switch, so any language switched off earlier comes back.
UPDATE installations SET enabled = 1 WHERE component IN ('php', 'go', 'rust', 'ruby', 'python');
