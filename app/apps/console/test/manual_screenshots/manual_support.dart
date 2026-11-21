// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Shared pieces of the manual screenshot suite (tag `manual_screenshots`, never run in CI).
// Generate with `scripts/manual/shots.sh`; PNGs land in docs/manual/img/<lang>/<id>.png.

import 'package:console/main.dart';
import 'package:console/state.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import '../support/console_host.dart';

/// Whole console window: wide enough for the side menu (>= 900), narrow enough to read in print.
const desktop = ShotDevice('desktop', 1680, 1080, 1.5);

/// One card or dialog on its own, at a column width the setup page really uses.
const cardView = ShotDevice('card', 1600, 2200, 2);

/// Phone width for the console drawer.
const consolePhone = ShotDevice('phone', 780, 1688, 2);

/// `--dart-define=SHOT_OUT=<docs/manual/img>`; `SHOT_ONLY=id1,id2` renders only those shots.
final manualOut = const String.fromEnvironment(
  'SHOT_OUT',
  defaultValue: 'build/manual_shots',
);
final _only = const String.fromEnvironment('SHOT_ONLY');
bool wanted(String id) => _only.isEmpty || _only.split(',').contains(id);

/// What one shot shows and how to get there.
class ManualShot {
  const ManualShot(
    this.id, {
    required this.build,
    this.device = desktop,
    this.act,
    this.signedIn = true,
    this.api,
  });

  final String id;

  /// The widget tree to pump (a whole app or a single screen in a host).
  final Widget Function(TestEnv env) build;
  final ShotDevice device;
  final bool signedIn;

  /// Routes this shot needs on top of (and ahead of) the shared ones.
  final void Function(FakeApi f)? api;

  /// Taps and scrolls after the first settle.
  final Future<void> Function(WidgetTester tester, T t)? act;
}

/// The real console root: sign-in gate, shell, side menu.
Widget consoleApp(TestEnv env) => ConsoleApp(
  session: env.session,
  state: ConsoleState(env.api, FakeRealtime(env.api), env.session),
  locale: env.locale,
  theme: env.theme,
);

/// One screen inside the console theme and localisation, no shell.
Widget consoleScreen(TestEnv env, Widget home) => ConsoleTestHost(
  env: env,
  state: ConsoleState(env.api, FakeRealtime(env.api), env.session),
  home: home,
);

/// Opens a console section through the Ctrl+K palette (the side menu scrolls; the palette never does).
Future<void> openSection(WidgetTester tester, String label) async {
  final t = lookupT(
    Localizations.localeOf(tester.element(find.byType(Scaffold).first)),
  );
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await shotSettle(tester);
  await tester.enterText(
    find.byWidgetPredicate(
      (w) => w is TextField && w.decoration?.hintText == t.cPaletteHint,
    ),
    label,
  );
  await shotSettle(tester);
  await tester.tap(find.widgetWithText(ListTile, label).last);
  await shotSettle(tester, duration: const Duration(seconds: 3));
}

/// Taps the first widget showing [text] and lets the result settle.
Future<void> tapText(WidgetTester tester, String text) async {
  await tester.tap(find.text(text).first, warnIfMissed: false);
  await shotSettle(tester);
}

/// Scrolls every list around [finder] so it sits near the top of the view. A lazy list builds
/// [finder] only once it is near the screen, so [within] names the screen whose list to drag first.
Future<void> scrollTo(
  WidgetTester tester,
  Finder finder, {
  double alignment = 0.05,
  Type? within,
}) async {
  if (finder.evaluate().isEmpty && within != null) {
    await tester.scrollUntilVisible(
      finder,
      400,
      scrollable: find
          .descendant(
            of: find.byType(within),
            matching: find.byType(Scrollable),
          )
          .first,
    );
  }
  await Scrollable.ensureVisible(
    tester.element(finder.first),
    alignment: alignment,
  );
  await shotSettle(tester);
}

/// A single card or panel on a plain page, the way the setup list shows it.
Widget cardHost(TestEnv env, Widget card) => consoleScreen(
  env,
  ListView(padding: const EdgeInsets.all(24), children: [card]),
);

/// Renders every shot in [shots] for each language, with a fresh fake backend per shot.
void runManualShots(
  String group,
  List<ManualShot> shots,
  void Function(FakeApi f) base,
) {
  for (final lang in const ['ko', 'en']) {
    testWidgets('manual shots ($group, $lang)', (tester) async {
      await tester.runAsync(loadShotFonts);
      final t = lookupT(Locale(lang));
      const nl = '\n';
      final failed = <String>[];
      for (final shot in shots.where((s) => wanted(s.id))) {
        final fake = FakeApi();
        shot.api?.call(fake);
        base(fake);
        final env = await TestEnv.create(
          fake: fake,
          localeCode: lang,
          themeMode: ThemeMode.light,
          role: 'staff_admin',
          signedIn: shot.signedIn,
        );
        try {
          await captureShot(
            tester,
            shot.build(env),
            shot.device,
            '$manualOut/$lang/${shot.id}.png',
            act: shot.act == null ? null : () => shot.act!(tester, t),
          );
        } catch (e) {
          failed.add('${shot.id}: ${'$e'.split(nl).first}');
        }
        final late = tester.takeException();
        if (late != null) failed.add('${shot.id}: ${'$late'.split(nl).first}');
        await unmount(tester);
      }
      // One list at the end, so a broken screen does not hide the rest of the run.
      expect(failed, isEmpty, reason: failed.join(nl));
    });
  }
}
