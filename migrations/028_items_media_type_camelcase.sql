-- Remap legacy media_type codes to camelCase strings (serialized MediaType)
UPDATE items
SET media_type = CASE media_type
    WHEN '' THEN 'all'
    WHEN 'u' THEN 'unknown'
    WHEN 'b' THEN 'printedText'
    WHEN 'm' THEN 'multimedia'
    WHEN 'bc' THEN 'comics'
    WHEN 'p' THEN 'periodic'
    WHEN 'v' THEN 'video'
    WHEN 'vt' THEN 'videoTape'
    WHEN 'vd' THEN 'videoDvd'
    WHEN 'a' THEN 'audio'
    WHEN 'am' THEN 'audioMusic'
    WHEN 'amt' THEN 'audioMusicTape'
    WHEN 'amc' THEN 'audioMusicCd'
    WHEN 'an' THEN 'audioNonMusic'
    WHEN 'ant' THEN 'audioNonMusicTape'
    WHEN 'anc' THEN 'audioNonMusicCd'
    WHEN 'c' THEN 'cdRom'
    WHEN 'i' THEN 'images'
    ELSE media_type
END
WHERE media_type IS NOT NULL
  AND media_type IN ('', 'u', 'b', 'm', 'bc', 'p', 'v', 'vt', 'vd', 'a', 'am', 'amt', 'amc', 'an', 'ant', 'anc', 'c', 'i');

