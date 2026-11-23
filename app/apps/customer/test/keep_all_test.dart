// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:customer/main.dart';
import 'package:customer/state.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/customer_routes.dart';
import 'support/golden_fonts.dart';

const _phone = Size(360, 780);
const _joiner = '\u2060';
final _space = RegExp(r'\s');

/// Each spot where [p] starts a line right after a non-space: a word split in two ("그대|로"). A word wider than
/// the whole line has to split somewhere, so it is not counted.
List<String> _midWordBreaks(RenderParagraph p) {
  final text = p.text.toPlainText(includeSemanticsLabels: false);
  double width(int from, int to) => p
      .getBoxesForSelection(TextSelection(baseOffset: from, extentOffset: to))
      .fold(0, (sum, b) => sum + b.right - b.left);
  final splits = <String>[];
  var wordStart = 0;
  double? lineTop;
  for (var i = 0; i < text.length; i++) {
    if (i > 0 && _space.hasMatch(text[i - 1])) wordStart = i;
    final boxes = p.getBoxesForSelection(
      TextSelection(baseOffset: i, extentOffset: i + 1),
    );
    if (boxes.isEmpty) continue;
    final top = boxes.first.top;
    if (lineTop != null && top > lineTop + 1 && wordStart != i) {
      final end = text.indexOf(_space, i);
      if (width(wordStart, end < 0 ? text.length : end) <= p.size.width) {
        splits.add(
          '${text.substring(0, i)}|${text.substring(i)}'.replaceAll(
            _joiner,
            '',
          ),
        );
      }
    }
    lineTop = top;
  }
  return splits;
}

/// The laid-out paragraphs under [of] that hold more than one word.
Iterable<RenderParagraph> _paragraphs(Finder of) sync* {
  for (final e
      in find.descendant(of: of, matching: find.byType(RichText)).evaluate()) {
    if (e.renderObject case final RenderParagraph p
        when p.attached &&
            p.hasSize &&
            p.text.toPlainText().trim().contains(_space)) {
      yield p;
    }
  }
}

Future<T> _pumpHome(WidgetTester tester, String lang) async {
  await loadGoldenFonts(tester);
  setViewport(tester, _phone);
  final fake = FakeApi();
  stockCustomerApi(fake);
  final env = await TestEnv.create(fake: fake, localeCode: lang);
  final brand = BrandController(
    api: env.api,
    fallback: BrandConfig.placeholder,
  );
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

/// [child] alone on a 360dp phone in [lang], [width] wide, with the real fonts.
Future<void> _pumpAt(
  WidgetTester tester,
  String lang,
  double width,
  Widget child,
) => tester.pumpWidget(
  MaterialApp(
    locale: Locale(lang),
    supportedLocales: const [Locale('ko'), Locale('en')],
    localizationsDelegates: const [
      T.delegate,
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    theme: buildTheme(BrandConfig.placeholder, Brightness.light),
    home: Scaffold(
      body: Align(
        alignment: Alignment.topLeft,
        child: SizedBox(width: width, child: child),
      ),
    ),
  ),
);

RenderParagraph _only(WidgetTester tester) =>
    tester.renderObject<RenderParagraph>(find.byType(RichText));

/// The copy the bug was reported on (home trust card, service notes) plus longer notes, banners and dialogs.
List<String> _samples(T t) => [
  t.trustVat,
  ...t.serviceNotes('aircon'),
  ...t.serviceNotes('bathroom'),
  ...t.serviceNotes('mattress'),
  t.apPgDevNote,
  t.cgDemoClearBody,
  t.guestSheetBody,
  t.pLogoutBody,
];

/// Card widths from a third of a 360dp phone up to its full content width (360 less 16dp gutters).
final _widths = [for (var w = 96.0; w <= 328; w += 8) w];

void main() {
  testWidgets('[ko 360x780] no home text splits a Korean word across lines', (
    tester,
  ) async {
    final t = await _pumpHome(tester, 'ko');
    final home = find.byType(MaterialApp);
    final multiLine = _paragraphs(home).where((p) => p.size.height > 30);

    expect(find.text(t.trustVat), findsOneWidget);
    expect(multiLine, isNotEmpty, reason: 'the home shows wrapped copy');
    expect([for (final p in _paragraphs(home)) ..._midWordBreaks(p)], isEmpty);
  });

  testWidgets(
    '[ko 360dp] the reported strings wrap only at spaces at every card width',
    (tester) async {
      await loadGoldenFonts(tester);
      setViewport(tester, _phone);
      final t = lookupT(const Locale('ko'));
      final kept = <String>[];
      final plain = <String>[];
      for (final s in _samples(t)) {
        for (final w in _widths) {
          await _pumpAt(tester, 'ko', w, KeepAllText(s));
          kept.addAll(_midWordBreaks(_only(tester)));
          await _pumpAt(tester, 'ko', w, Text(s));
          plain.addAll(_midWordBreaks(_only(tester)));
        }
      }
      expect(kept, isEmpty);
      // The same strings in a plain Text do split mid-word, so the check above can see the bug.
      expect(plain, contains(contains('그대|로')));
      expect(plain, contains(contains('않아|요')));
    },
  );

  testWidgets(
    '[en 360dp] English text is drawn and wrapped exactly as a plain Text',
    (tester) async {
      await loadGoldenFonts(tester);
      setViewport(tester, _phone);
      final t = lookupT(const Locale('en'));
      for (final s in _samples(t)) {
        for (final w in _widths) {
          await _pumpAt(tester, 'en', w, Text(s));
          final plainHeight = _only(tester).size.height;
          await _pumpAt(tester, 'en', w, KeepAllText(s));
          final p = _only(tester);
          expect(p.text.toPlainText(), s);
          expect(p.size.height, plainHeight, reason: '"$s" at ${w}dp');
        }
      }
    },
  );

  testWidgets(
    '[ko] screen readers, finders and copied text get the plain string',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final t = lookupT(const Locale('ko'));
      final s = t.apPgDevNote;

      await _pumpAt(tester, 'ko', 200, KeepAllText(s));
      expect(
        _only(tester).text.toPlainText(includeSemanticsLabels: false),
        contains(_joiner),
      );
      expect(find.text(s), findsOneWidget);
      expect(find.bySemanticsLabel(s), findsOneWidget);

      // Selectable text is drawn unjoined, so a copy never carries the invisible joiners.
      await _pumpAt(tester, 'ko', 200, SelectionArea(child: KeepAllText(s)));
      expect(_only(tester).text.toPlainText(includeSemanticsLabels: false), s);
      semantics.dispose();
    },
  );

  test('keepAll joins only inside Korean words of multi-word text', () {
    expect(keepAll('그대로 남아요'), '그$_joiner대$_joiner로 남$_joiner아$_joiner요');
    expect(
      keepAll('AI가 답변해요.'),
      'AI$_joiner가 답$_joiner변$_joiner해$_joiner요$_joiner.',
    );
    expect(keepAll('그대로'), '그대로', reason: 'one word has nowhere else to break');
    expect(keepAll('Kept as shown'), 'Kept as shown');
    expect(
      keepAll('https://uzipsa.co.kr 12,000'),
      'https://uzipsa.co.kr 12,000',
    );
    expect(keepAll('첫 줄\n둘째 줄'), '첫 줄\n둘$_joiner째 줄');
  });
}
