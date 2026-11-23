// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:customer/screens/flow_widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

final _themes = {
  for (final b in Brightness.values) b: buildTheme(BrandConfig.placeholder, b),
};

void main() {
  // The move flow's line icons (type_*, bld_*, elev_*) are white masks: on the
  // filled primary circle they take onPrimary, elsewhere the ink default.
  testWidgets('a selected circle paints its mono icon onPrimary', (
    tester,
  ) async {
    for (final b in Brightness.values) {
      final theme = _themes[b]!;
      await tester.pumpWidget(
        MaterialApp(
          theme: theme,
          // No theme lerp: the dark pass must paint dark colours at once.
          themeAnimationDuration: Duration.zero,
          home: Scaffold(
            body: Row(
              children: [
                CircleChoice(
                  icon: 'elev_yes',
                  label: 'yes',
                  selected: true,
                  onTap: () {},
                ),
                CircleChoice(
                  icon: 'elev_no',
                  label: 'no',
                  selected: false,
                  onTap: () {},
                ),
              ],
            ),
          ),
        ),
      );
      Icon3D icon(String id) => tester.widget<Icon3D>(
        find.byWidgetPredicate((w) => w is Icon3D && w.id == id),
      );
      expect(icon('elev_yes').color, theme.colorScheme.onPrimary, reason: '$b');
      // Unselected: no override, so Icon3D falls back to the theme ink.
      expect(icon('elev_no').color, isNull, reason: '$b');
      final art = tester.widget<Image>(
        find.descendant(
          of: find.byKey(const ValueKey('icon3d:elev_yes')),
          matching: find.byType(Image),
        ),
      );
      expect(art.color, theme.colorScheme.onPrimary, reason: '$b');
      // The glyph clears the 3:1 non-text contrast bar on the circle it sits on.
      expect(
        contrastRatio(theme.colorScheme.onPrimary, theme.colorScheme.primary),
        greaterThanOrEqualTo(3),
        reason: '$b',
      );
    }
  });
}
