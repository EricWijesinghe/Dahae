import 'package:console/nav_sections.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

/// Each sidebar row must draw the 3D art and glyph of its own key (K-06: no one-row shift).
const _expectedIcons = <String, IconData>{
  'c_chat': Icons.forum_rounded,
  'c_pricing': Icons.sell_rounded,
  'c_marketing': Icons.campaign_rounded,
  'c_partners': Icons.groups_rounded,
  'c_payments': Icons.payments_rounded,
  'c_payouts': Icons.account_balance_wallet_rounded,
  'c_subscriptions': Icons.autorenew_rounded,
  'c_customers': Icons.people_rounded,
  'c_staff': Icons.badge_rounded,
  'c_settings': Icons.settings_rounded,
  'c_monitor': Icons.monitor_heart_rounded,
};

void main() {
  test('every sidebar row has a unique registry key', () {
    final ids = consoleSections.map((s) => s.id).toList();
    expect(ids.toSet().length, ids.length, reason: 'duplicate row id');
    for (final id in ids) {
      expect(
        Icon3DRegistry.specs.containsKey(id),
        isTrue,
        reason: 'row $id has no entry in icon3d_registry.dart',
      );
    }
  });

  test('each named row draws the icon of its own label', () {
    for (final entry in _expectedIcons.entries) {
      final section = consoleSections.singleWhere((s) => s.id == entry.key);
      expect(section.icon, entry.value, reason: entry.key);
    }
  });
}
