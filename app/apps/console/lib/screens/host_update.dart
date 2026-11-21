// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import '../widgets/owner_totp_dialog.dart';
import 'host_controls.dart';

/// "Update now" for the owner, plus the recent server jobs with their state. The button needs the owner's
/// code for each press and is hidden from other admins; a run already queued or running turns it off.
class HostUpdateCard extends StatefulWidget {
  const HostUpdateCard({super.key});

  @override
  State<HostUpdateCard> createState() => _HostUpdateCardState();
}

class _HostUpdateCardState extends State<HostUpdateCard> {
  // True from the code prompt until the request is answered: a second tap cannot queue a second update.
  bool _sending = false;

  @override
  Widget build(BuildContext context) => AsyncView<Json>(
    load: () async {
      final api = context.api;
      return {
        'me': await api.get('/admin/me/account') as Json,
        'jobs': await api.get('/admin/host/jobs') as Json,
      };
    },
    builder: (context, data, reload) {
      final t = context.t;
      final isOwner = (data['me'] as Json)['is_owner'] == true;
      final jobs = ((data['jobs'] as Json)['jobs'] as List? ?? const [])
          .cast<Json>();
      final busy = jobs.any(
        (j) =>
            j['kind'] == 'update' &&
            (j['state'] == 'queued' || j['state'] == 'running'),
      );
      return AppCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(t.hjTitle, style: context.text.titleSmall),
            const SizedBox(height: Space.xs),
            Text(t.hjBody, style: context.text.bodyMedium),
            if (isOwner) ...[
              const SizedBox(height: Space.sm),
              PrimaryButton(
                label: t.hjUpdateNow,
                onPressed: busy || _sending
                    ? null
                    : () => _update(context, reload),
              ),
              const SizedBox(height: Space.lg),
              HostControls(onQueued: reload),
              const SizedBox(height: Space.lg),
              const ApprovalEnrollCard(),
            ],
            const SizedBox(height: Space.md),
            Text(t.hjRecent, style: context.text.labelLarge),
            if (jobs.isEmpty)
              Text(t.hjEmpty, style: context.text.bodySmall)
            else
              for (final j in jobs.take(5))
                Padding(
                  padding: const EdgeInsets.only(top: Space.xs),
                  child: Text(
                    '${_kind(t, j['kind'] as String)} · ${_state(t, j['state'] as String)}'
                    '${j['step'] is String ? ' · ${j['step']}' : ''}',
                    style: context.text.bodySmall,
                  ),
                ),
          ],
        ),
      );
    },
  );

  Future<void> _update(
    BuildContext context,
    Future<void> Function() reload,
  ) async {
    setState(() => _sending = true);
    final Object? sent;
    try {
      sent = await runOwnerAction(
        context,
        (headers) => context.api.post(
          '/admin/host/jobs',
          {'kind': 'update', 'arg': ''},
          ApiClient.newIdempotencyKey(),
          headers,
        ),
        done: context.t.hjQueued,
      );
    } finally {
      if (mounted) setState(() => _sending = false);
    }
    if (sent != null) await reload();
  }
}

String _kind(T t, String kind) => switch (kind) {
  'update' => t.hjKindUpdate,
  'status-refresh' => t.cRbKindStatusRefresh,
  'backup-now' => t.cRbKindBackupNow,
  'update-skip' => t.cRbKindUpdateSkip,
  'channel' => t.cRbKindChannel,
  'restart' => t.cRbKindRestart,
  'schedule' => t.cRbKindSchedule,
  'heartbeat' => t.cRbKindHeartbeat,
  'domain-check' => t.cRbKindDomainCheck,
  'domain-set' => t.cRbKindDomainSet,
  'host-approval-enroll' => t.cRbKindEnroll,
  _ => kind,
};

String _state(T t, String state) => switch (state) {
  'queued' => t.hjStateQueued,
  'running' => t.hjStateRunning,
  'ok' => t.hjStateOk,
  'failed' => t.hjStateFailed,
  'rolled_back' => t.hjStateRolledBack,
  'awaiting_approval' => t.cRbStateAwaiting,
  'rejected' => t.cRbStateRejected,
  _ => t.hjStateCheck,
};
