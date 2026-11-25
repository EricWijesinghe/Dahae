// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:customer/screens/flow_widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

/// DODA's split: dark teal for text, a light sky fill for filled controls.
const _sky = BrandConfig(
  name: '우집사',
  primary: Color(0xFF00838F),
  primaryDeep: Color(0xFF006A74),
  fresh: Color(0xFF1E9EBA),
  sun: Color(0xFFFFD95A),
  fill: Color(0xFF00BCD4),
);

final _themes = [
  for (final brand in [BrandConfig.placeholder, _sky])
    for (final b in Brightness.values) (b, buildTheme(brand, b)),
];

void main() {
  // The move flow's line icons (type_*, bld_*, elev_*) are white masks: on the
  // circle filled with the owner's fill colour they take its label colour, elsewhere the ink default.
  testWidgets('a selected circle is the fill colour with its mono icon onFill', (
    tester,
  ) async {
    for (final (b, theme) in _themes) {
      final c = theme.extension<AppColors>()!;
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
      expect(icon('elev_yes').color, c.onFill, reason: '$b');
      // Unselected: no override, so Icon3D falls back to the theme ink.
      expect(icon('elev_no').color, isNull, reason: '$b');
      final art = tester.widget<Image>(
        find.descendant(
          of: find.byKey(const ValueKey('icon3d:elev_yes')),
          matching: find.byType(Image),
        ),
      );
      expect(art.color, c.onFill, reason: '$b');
      final disc = tester.widget<AnimatedContainer>(
        find
            .ancestor(
              of: find.byKey(const ValueKey('icon3d:elev_yes')),
              matching: find.byType(AnimatedContainer),
            )
            .first,
      );
      expect((disc.decoration! as BoxDecoration).color, c.brandStart);
      // The glyph clears the 3:1 non-text contrast bar on the circle it sits on.
      expect(
        contrastRatio(c.onFill, c.brandStart),
        greaterThanOrEqualTo(3),
        reason: '$b',
      );
    }
  });
}
