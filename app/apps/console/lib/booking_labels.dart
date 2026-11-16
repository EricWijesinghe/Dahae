// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';

/// The customer's name, or the 이름 없음 label when the record has none (K-07).
String customerNameLabel(T t, String? name) {
  final trimmed = name?.trim() ?? '';
  return trimmed.isEmpty ? t.nameNone : trimmed;
}

/// Avatar letter for a customer name; '?' when there is no name.
String customerInitial(String? name) {
  final trimmed = name?.trim() ?? '';
  return trimmed.isEmpty ? '?' : trimmed.characters.first;
}

/// Counts line of a counts booking, e.g. "작은 창 3 · 큰 창 2" (CR-8.3).
/// [services] is the raw `/admin/services` list. Null when the booking has no counts.
String? bookingCountsLine(T t, Booking b, List<Json> services) {
  if (b.unitCounts.isEmpty) return null;
  final match = services.where((s) => s['code'] == b.serviceCode);
  if (match.isEmpty) return null;
  final service = Service.fromJson(match.first);
  final line = t.unitCounts(service, b.countsOf(service.units.length));
  return line.isEmpty ? null : line;
}
