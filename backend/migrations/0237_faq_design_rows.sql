-- FAQ rows from the design (p27), with the category codes the FAQ chips filter on (booking, payment, move).
-- A row is added only when no FAQ with the same Korean question exists, so owner edits and deletions of
-- these rows are not undone by a later run.
INSERT INTO content_items (kind, title, body, sort_order)
SELECT 'faq', t.title::jsonb, t.body::jsonb, t.sort_order
FROM (VALUES
    ('예약은 언제까지 변경할 수 있나요?',
     '{"ko": "예약은 언제까지 변경할 수 있나요?", "en": "How far in advance can I change my booking?"}',
     '{"category": "booking", "text": {"ko": "방문 2일 전까지 무료로 취소할 수 있어요. 일정 변경이 가능한지는 예약 상세 화면에서 확인해 주세요.", "en": "You can cancel for free up to 2 days before the visit. Check the booking details screen to see whether the date can be changed."}}',
     1),
    ('매니저님이 청소도구를 가져오나요?',
     '{"ko": "매니저님이 청소도구를 가져오나요?", "en": "Does the manager bring cleaning supplies?"}',
     '{"category": "booking", "text": {"ko": "원룸·화장실·주방 청소는 고객님 댁의 기본 청소도구를 사용해요. 이사·입주, 에어컨, 매트리스, 소파 청소는 전문 장비를 가져갑니다.", "en": "Studio, bathroom and kitchen cleaning uses the basic cleaning tools at your home. Move-in and move-out, aircon, mattress and sofa cleaning come with professional equipment."}}',
     2),
    ('할인 쿠폰은 어떻게 받나요?',
     '{"ko": "할인 쿠폰은 어떻게 받나요?", "en": "How do I get discount coupons?"}',
     '{"category": "payment", "text": {"ko": "신규 고객 웰컴 쿠폰팩은 홈 화면의 웰컴 혜택 배너에서 받을 수 있어요. 받은 쿠폰은 결제 화면에서 선택해 적용해요.", "en": "New customers can get the welcome coupon pack from the welcome banner on the home screen. Choose a coupon you have received on the payment screen to apply it."}}',
     3),
    ('이사·입주 청소는 왜 상담이 필요한가요?',
     '{"ko": "이사·입주 청소는 왜 상담이 필요한가요?", "en": "Why does move-in and move-out cleaning need a consultation?"}',
     '{"category": "move", "text": {"ko": "집 구조, 짐 상태, 층수와 엘리베이터 여부에 따라 작업량이 크게 달라져요. 상담으로 정확한 견적을 확인한 뒤 예약금을 결제해요.", "en": "The amount of work depends on the layout, how full the home is, the floor and whether there is an elevator. We confirm an exact quote in a consultation, and you pay the deposit after that."}}',
     4)
) AS t (question, title, body, sort_order)
WHERE NOT EXISTS (
    SELECT 1 FROM content_items c
    WHERE c.kind = 'faq' AND c.title->>'ko' = t.question
);
