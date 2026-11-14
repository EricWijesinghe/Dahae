-- Mattress legacy data. The old unit_index n meant n+1 mattresses (count_1..count_4). Once the mattress
-- service is counts pricing, each open or past row keeps its quantity as unit_counts = {n+1}. unit_index is
-- kept as it was (history stays intact; counts win when both are present). Only applies while the mattress
-- service is counts pricing, so an owner-edited unit-priced mattress keeps its legacy unit_index.
UPDATE bookings b SET unit_counts = ARRAY[(b.unit_index + 1)::smallint]
  FROM services s
 WHERE s.code = 'mattress' AND s.pricing->>'type' = 'counts'
   AND b.service_code = 'mattress' AND b.unit_index IS NOT NULL AND b.unit_counts IS NULL;

UPDATE subscriptions sub SET unit_counts = ARRAY[(sub.unit_index + 1)::smallint]
  FROM services s
 WHERE s.code = 'mattress' AND s.pricing->>'type' = 'counts'
   AND sub.service_code = 'mattress' AND sub.unit_index IS NOT NULL AND sub.unit_counts IS NULL;
