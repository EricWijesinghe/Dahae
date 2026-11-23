// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/people.dart';
import 'package:core/core.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

Future<void> _pump(WidgetTester tester, List<Json> pros) async {
  setViewport(tester, wideSize);
  final fake = FakeApi()
    ..reply('GET', '/admin/team-members/pending', const <Json>[])
    ..reply('GET', '/admin/pros', pros);
  stockConsoleApi(fake);
  final env = await TestEnv.create(fake: fake, role: 'staff_admin');
  await pumpConsoleScreen(tester, env, const PartnersScreen());
  await settle(tester);
}

void main() {
  testWidgets('no partners yet says they join through the partner app', (
    tester,
  ) async {
    await _pump(tester, const []);
    expect(find.text(koT.cProsEmpty), findsOneWidget);
    expect(find.text(koT.cProsEmptyBody), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('the fee reads as a plain percentage', (tester) async {
    await _pump(tester, [
      {
        'id': 'p1',
        'team_name': '말끔팀',
        'name': '김하나',
        'phone': '01012345678',
        'status': 'pending',
        'completed': 3,
        'cancelled': 0,
        'rating': 4.8,
        'fee_bps': 2000,
        'quality_score': null,
        'quality_status': null,
      },
    ]);
    expect(find.textContaining(koT.cFeeRate('20')), findsOneWidget);
    await unmount(tester);
  });
}
