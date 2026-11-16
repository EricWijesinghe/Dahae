// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:console/screens/stepup.dart';
import 'package:console/state.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/console_host.dart';

const _deletePath = '/admin/privacy/customers/c-1/delete';
const _stepUpPath = '/auth/staff/mfa/step-up';

/// A button that runs one sensitive call through [withStepUp]. Errors that escape are recorded, not thrown.
Widget _probe(List<Object> escaped) => Center(
  child: Builder(
    builder: (ctx) => TextButton(
      onPressed: () async {
        try {
          await withStepUp(
            ctx,
            () => ctx.api.post(_deletePath, {'phone': '01012345678'}),
          );
        } on ApiException catch (e) {
          escaped.add(e);
        }
      },
      child: const Text('run'),
    ),
  ),
);

FakeApi _fakeWithStepUpRequired(int Function() attempts) {
  return FakeApi()
    ..on('POST', _deletePath, (_) {
      return attempts() == 1
          ? const ApiReply(403, {
              'error': {'code': 'stepup_required'},
            })
          : const ApiReply(200, {'ok': true});
    })
    ..reply('POST', _stepUpPath, {'ok': true, 'valid_minutes': 5});
}

void main() {
  testWidgets(
    'a stepup_required answer asks for a code, then retries the action once',
    (tester) async {
      setViewport(tester, tallPhoneSize);
      var attempts = 0;
      final fake = _fakeWithStepUpRequired(() => ++attempts);
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      await pumpConsoleScreen(tester, env, _probe([]));

      await tester.tap(find.text('run'));
      await pumpUntilFound(tester, find.text(koT.cStepUpTitle));
      await tester.enterText(find.byType(TextField), '123456');
      await tester.tap(find.text(koT.cStepUpConfirm));
      await settle(tester);

      expect(fake.callsTo('POST', _stepUpPath).single.body, {'code': '123456'});
      expect(fake.callsTo('POST', _deletePath), hasLength(2));
      expect(find.text(koT.cStepUpTitle), findsNothing);
      await unmount(tester);
    },
  );

  testWidgets(
    'cancelling the prompt sends nothing and keeps the action refused',
    (tester) async {
      setViewport(tester, tallPhoneSize);
      var attempts = 0;
      final fake = _fakeWithStepUpRequired(() => ++attempts);
      final escaped = <Object>[];
      final env = await TestEnv.create(fake: fake, role: 'staff_admin');
      await pumpConsoleScreen(tester, env, _probe(escaped));

      await tester.tap(find.text('run'));
      await pumpUntilFound(tester, find.text(koT.cStepUpTitle));
      await tester.tap(find.text(koT.cStepUpCancel));
      await settle(tester);

      expect(fake.callsTo('POST', _stepUpPath), isEmpty);
      expect(fake.callsTo('POST', _deletePath), hasLength(1));
      expect(escaped.single, isA<ApiException>());
      await unmount(tester);
    },
  );
}
