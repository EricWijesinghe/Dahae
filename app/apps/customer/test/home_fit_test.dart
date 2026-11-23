// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:customer/main.dart';
import 'package:customer/state.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/customer_routes.dart';
import 'support/golden_fonts.dart';

/// The live brand: a three-syllable Korean name and its English name, longer than the placeholder.
const _brand = BrandConfig(
  name: '우집사',
  nameEn: 'Uzipsa',
  primary: Color(0xFF187A8F),
  primaryDeep: Color(0xFF12606F),
  fresh: Color(0xFF1E9EBA),
  sun: Color(0xFFFFD95A),
);

const _phone = Size(360, 780);

/// Every laid-out text under [of] that was cut to fit (ellipsis or line limit).
List<String> _cut(WidgetTester tester, Finder of) => [
  for (final e
      in find.descendant(of: of, matching: find.byType(RichText)).evaluate())
    if (e.renderObject case final RenderParagraph p
        when p.attached && p.hasSize && p.didExceedMaxLines)
      p.text.toPlainText(),
];

Future<T> _pumpHome(
  WidgetTester tester,
  String lang, {
  bool signedIn = false,
}) async {
  // Real Dahae Sans metrics: the test font draws every glyph a full em wide.
  await loadGoldenFonts(tester);
  setViewport(tester, _phone);
  final fake = FakeApi();
  stockCustomerApi(fake);
  final env = await TestEnv.create(
    fake: fake,
    localeCode: lang,
    signedIn: signedIn,
  );
  final brand = BrandController(api: env.api, fallback: _brand);
  addTearDown(brand.dispose);
  await tester.pumpWidget(
    CustomerApp(
      session: env.session,
      app: AppState(env.api, FakeRealtime(env.api)),
      locale: env.locale,
      theme: env.theme,
      brand: brand,
    ),
  );
  final t = lookupT(Locale(lang));
  await pumpUntilFound(tester, find.text(t.tabHome));
  await settle(tester, duration: const Duration(milliseconds: 600));
  return t;
}

void main() {
  for (final lang in ['ko', 'en']) {
    final name = _brand.nameFor(lang);

    testWidgets('[$lang 360x780] the home toggle labels fit uncut', (
      tester,
    ) async {
      final t = await _pumpHome(tester, lang);
      final toggle = find.byWidgetPredicate((w) => w is SegmentedControl);

      expect(find.text(t.dhHomeAll), findsOneWidget);
      expect(find.text(t.dhHomePick(name)), findsOneWidget);
      expect(_cut(tester, find.byType(AppBar)), isEmpty);
      // Still readable: the labels shrink to fit but stay at 12sp or more.
      for (final label in [t.dhHomeAll, t.dhHomePick(name)]) {
        final p = tester.renderObject<RenderParagraph>(
          find.descendant(of: toggle, matching: find.text(label)),
        );
        expect(
          p.textScaler.scale(p.text.style!.fontSize!),
          greaterThanOrEqualTo(12),
        );
      }
      expect(tester.takeException(), isNull);
      await unmount(tester);
    });

    testWidgets('[$lang 360x780] nothing on home is cut, top to bottom', (
      tester,
    ) async {
      await _pumpHome(tester, lang);
      final cut = <String>{};
      for (var i = 0; i < 8; i++) {
        cut.addAll(_cut(tester, find.byType(MaterialApp)));
        await tester.dragFrom(const Offset(3, 400), const Offset(0, -300));
        await settle(tester, duration: const Duration(milliseconds: 300));
      }
      expect(cut, isEmpty);
      expect(tester.takeException(), isNull);
      await unmount(tester);
    });

    testWidgets(
      '[$lang 360x780] the More coupon and point strip stays readable',
      (tester) async {
        final t = await _pumpHome(tester, lang);
        await tester.tap(find.text(t.tabMore));
        await settle(tester, duration: const Duration(milliseconds: 600));

        // The profile card's coupon / point strip (other rows reuse the hint).
        final hints = find.descendant(
          of: find.ancestor(
            of: find.text(t.moreCoupons),
            matching: find.byType(IntrinsicHeight),
          ),
          matching: find.text(t.guestAccountHint),
        );
        expect(hints, findsNWidgets(2));
        for (final e in hints.evaluate()) {
          final p = e.renderObject! as RenderParagraph;
          final shown =
              p.textScaler.scale(p.text.style!.fontSize!) *
              _scaleOnScreen(tester, p);
          expect(shown, greaterThanOrEqualTo(11), reason: 'shown at $shown px');
        }
        expect(tester.takeException(), isNull);
        await unmount(tester);
      },
    );
  }
}

/// How much [p] is scaled by transforms above it (a FittedBox shrinks text without changing its style).
double _scaleOnScreen(WidgetTester tester, RenderParagraph p) {
  final m = p.getTransformTo(tester.renderObject(find.byType(Navigator).first));
  return MatrixUtils.transformRect(m, const Rect.fromLTWH(0, 0, 100, 0)).width /
      100;
}
