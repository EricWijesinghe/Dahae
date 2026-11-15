-- Welcome banner copy (design p01): headline, sub-line and the art card "웰컴 혜택 / 15,000 원".
-- Only rows still carrying a seeded title change; a banner the owner edited keeps its own words.
UPDATE content_items
SET title = '{"ko": "첫 청소 웰컴 쿠폰팩 즉시 지급", "en": "Welcome coupon pack for your first clean, issued now"}'::jsonb,
    body = body || '{
        "text": {"ko": "신규 고객 웰컴 쿠폰", "en": "Welcome coupon for new customers"},
        "card": {"label": {"ko": "웰컴 혜택", "en": "Welcome gift"}, "value": {"ko": "15,000 원", "en": "₩15,000"}}
    }'::jsonb
WHERE kind = 'banner'
  AND title->>'ko' IN ('첫 청소 15,000원 웰컴쿠폰', '첫 예약 쿠폰 2장 받기');
