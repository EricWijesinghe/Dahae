/// Shows a Korean phone number in its usual grouping: 010-1234-5678 (mobile, 11 digits),
/// 02-1234-5678 (Seoul, 2-digit area code) or 031-123-4567 (other landlines, 10 digits).
/// A +82 prefix is converted to the domestic form. Partial input formats as far as it can,
/// so a field can format while the user types.
String formatKoreanPhone(String raw) {
  var d = raw.replaceAll(RegExp(r'\D'), '');
  if (d.startsWith('82') && (d.length == 11 || d.length == 12)) {
    d = '0${d.substring(2)}';
  }
  final full = _group(d, d.startsWith('02') ? 2 : 3);
  if (full != null) return full;
  if (d.length < 4) return d;
  if (d.length < 8) return '${d.substring(0, 3)}-${d.substring(3)}';
  final mid = d.length == 11 ? 7 : 6;
  return '${d.substring(0, 3)}-${d.substring(3, mid)}-${d.substring(mid)}';
}

/// Groups [digits] as area-middle-last with an [areaLen]-digit area code. Returns null for
/// lengths that are not a 9-11 digit number.
String? _group(String digits, int areaLen) {
  final n = digits.length;
  if (n < 9 || n > 11) return null;
  final midLen = n <= 10 ? (n - areaLen) ~/ 2 : n - areaLen - 4;
  final mid = areaLen + midLen;
  return '${digits.substring(0, areaLen)}-${digits.substring(areaLen, mid)}-${digits.substring(mid)}';
}
