// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

/// The home group that holds [code] (CR-2). A service in no group is not on home.
ServiceGroup homeGroupOf(String code) =>
    defaultServiceGroups.firstWhere((g) => g.services.contains(code));

/// The title a home tile shows for the group that holds [code].
String homeGroupTitle(String code, {String lang = 'ko'}) =>
    homeGroupOf(code).titleFor(lang);

/// Taps the home tile of the group that holds [code] and lets the screen it opens settle.
Future<void> tapHomeGroup(
  WidgetTester tester,
  String code, {
  String lang = 'ko',
}) async {
  final title = homeGroupTitle(code, lang: lang);
  await pumpUntilFound(tester, find.text(title));
  await tester.tap(find.text(title));
  await settle(tester);
}

/// From home, opens the service [code]: its group tile, then the member row for a list group.
/// A direct group opens its service at once; a move group asks its questions first, so use
/// [tapHomeGroup] there.
Future<void> openServiceFromHome(WidgetTester tester, String code) async {
  await tapHomeGroup(tester, code);
  if (homeGroupOf(code).chooser != 'list') return;
  await pumpUntilFound(tester, find.text(koT.service(code)));
  await tester.tap(find.text(koT.service(code)));
  await settle(tester);
}
