import 'package:core/core.dart';

/// One counts option in the owner's catalogue order: its key and the owner's label (null for built-in options).
typedef _CountUnit = ({String key, String? label});

/// Original aircon options, used only for legacy bookings that carry answers.ac_addons and no
/// unit_counts (those rows have no catalogue order of their own).
/// shortcut: legacy aircon rows only, remove once those bookings are migrated to unit_counts.
const _legacyAirconKeys = ['wall', 'stand', 'two_in_one', 'ceiling'];

/// The job's counts options from the live catalogue (`count_units`); null for non-counts jobs.
List<_CountUnit>? _countUnits(Json job) {
  final units = job['count_units'];
  if (units is List) {
    return [
      for (final u in units.cast<Json>())
        (key: '${u['label_key']}', label: u['label'] as String?),
    ];
  }
  if (job['service_code'] != 'aircon') return null;
  return [for (final k in _legacyAirconKeys) (key: k, label: null)];
}

String _unitName(T t, _CountUnit u) => switch (u.key) {
  'wall' => t.pUnitWall,
  'stand' => t.pUnitStand,
  'two_in_one' => t.pUnitTwoInOne,
  'ceiling' => t.pUnitCeiling,
  'seat_1_2' => t.pUnitSeat12,
  'seat_3' => t.pUnitSeat3,
  'seat_4_plus' => t.pUnitSeat4Plus,
  'l_shape' => t.pUnitLShape,
  'mattress' => t.pUnitMattress,
  'window_small' => t.pUnitWindowSmall,
  'window_large' => t.pUnitWindowLarge,
  'pest' => t.pUnitPest,
  'mite_bedbug' => t.pUnitMiteBedbug,
  'sanitize' => t.pUnitSanitize,
  'sick_house' => t.pUnitSickHouse,
  _ => u.label ?? u.key,
};

/// Counts per option in catalogue order. Older aircon bookings carry only answers.ac_addons
/// (one unit/count entry per option), so those map onto the same order.
List<num>? _jobCounts(Json job, List<_CountUnit> units) {
  final counts = (job['unit_counts'] as List?)?.cast<num>();
  if (counts != null) return counts;
  final answers = job['answers'];
  final addons = answers is Map ? answers['ac_addons'] : null;
  if (addons is! List || addons.isEmpty) return null;
  final byUnit = <String, num>{
    for (final a in addons.whereType<Map<String, dynamic>>())
      if (a['unit'] is String) a['unit'] as String: (a['count'] as num?) ?? 0,
  };
  return [for (final u in units) byUnit[u.key] ?? 0];
}

/// "벽걸이 1 · 스탠드 2" for a job's counts; null when the job has none to show.
String? jobCountsLabel(T t, Json job) {
  final units = _countUnits(job);
  if (units == null) return null;
  final counts = _jobCounts(job, units);
  if (counts == null) return null;
  final parts = [
    for (var i = 0; i < counts.length && i < units.length; i++)
      if (counts[i] > 0) '${_unitName(t, units[i])} ${counts[i]}',
  ];
  return parts.isEmpty ? null : parts.join(' · ');
}

/// Home type a quote was answered with (원룸·투룸 / 아파트 / 빌라·주택); null for older quotes.
String? homeTypeLabel(T t, Json? answers) => switch (answers?['home_type']) {
  'one_two_room' => t.pHomeOneTwo,
  'apartment' => t.pHomeApartment,
  'villa_house' => t.pHomeVillaHouse,
  _ => null,
};
