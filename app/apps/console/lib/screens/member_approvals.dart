// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/inline_button.dart';

/// Team members a partner invited that the platform owner has not approved yet. Admins only.
class MemberApprovals extends StatelessWidget {
  const MemberApprovals({super.key});

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return AsyncView<List<Json>>(
      load: () async =>
          (await context.api.get('/admin/team-members/pending') as List)
              .cast<Json>(),
      builder: (context, rows, reload) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final m in rows)
            Card(
              elevation: 0,
              margin: const EdgeInsets.symmetric(
                horizontal: Space.lg,
                vertical: Space.xs,
              ),
              child: ListTile(
                title: Text(
                  '${m['team_name']} · ${m['name']}',
                  style: context.text.titleSmall,
                ),
                subtitle: Text('${t.cPro_pending} · ${m['phone']}'),
                trailing: FilledButton.tonal(
                  style: compactFilled,
                  onPressed: () => _approve(context, '${m['id']}', reload),
                  child: Text(t.cVerify),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

Future<void> _approve(
  BuildContext context,
  String memberId,
  Future<void> Function() reload,
) async {
  final t = context.t;
  try {
    await context.api.post('/admin/team-members/$memberId/approve');
    if (context.mounted) showToast(context, t.cDone);
    await reload();
  } catch (e) {
    if (context.mounted) showToast(context, t.error(e));
  }
}
