// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../host_job.dart';
import '../state.dart';
import '../widgets/copy_row.dart';
import '../widgets/owner_totp_dialog.dart';
import 'confirm_action.dart';

const _days = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
final _hhmm = RegExp(r'^([01]\d|2[0-3]):[0-5]\d$');

/// Queues one owner job behind the owner's code; resolves true when it was queued.
Future<bool> sendOwnerJob(BuildContext context, String kind, String arg) async {
  final api = context.api;
  final sent = await runOwnerAction(
    context,
    (h) => queueHostJob(api, kind, arg, h),
    done: context.t.cRbJobQueued,
  );
  return sent != null;
}

/// Owner-only server settings that used to need a terminal: update channel, update schedule, backup,
/// restart and skip. Each sends one server job (owner code each time); [onQueued] refreshes the job list.
class HostControls extends StatefulWidget {
  const HostControls({super.key, required this.onQueued});
  final Future<void> Function() onQueued;

  @override
  State<HostControls> createState() => _HostControlsState();
}

class _HostControlsState extends State<HostControls> {
  String _channel = 'stable';
  String _day = 'mon';
  final _time = TextEditingController(text: '04:00');
  bool _badTime = false;

  @override
  void dispose() {
    _time.dispose();
    super.dispose();
  }

  Future<void> _send(String kind, String arg) async {
    if (await sendOwnerJob(context, kind, arg)) await widget.onQueued();
  }

  void _schedule() {
    final time = _time.text.trim();
    if (_day != 'off' && !_hhmm.hasMatch(time)) {
      setState(() => _badTime = true);
      return;
    }
    setState(() => _badTime = false);
    _send('schedule', _day == 'off' ? 'off' : '$_day@$time');
  }

  Future<void> _restart() async {
    final t = context.t;
    if (await confirmAction(
      context,
      t.cRbRestart,
      message: t.cRbRestartConfirm,
    )) {
      if (mounted) await _send('restart', 'all');
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final dayNames = t.cRbDays.split(',');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(t.cRbHostTitle, style: context.text.labelLarge),
        const SizedBox(height: Space.sm),
        Text(t.cRbChannel, style: context.text.bodyMedium),
        Wrap(
          spacing: Space.sm,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            SegmentedButton<String>(
              segments: [
                ButtonSegment(value: 'stable', label: Text(t.cRbChannelStable)),
                ButtonSegment(value: 'beta', label: Text(t.cRbChannelBeta)),
              ],
              selected: {_channel},
              onSelectionChanged: (s) => setState(() => _channel = s.first),
            ),
            OutlinedButton(
              onPressed: () => _send('channel', _channel),
              child: Text(t.cRbApply),
            ),
          ],
        ),
        const SizedBox(height: Space.md),
        Text(t.cRbSchedule, style: context.text.bodyMedium),
        Wrap(
          spacing: Space.sm,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            DropdownButton<String>(
              value: _day,
              items: [
                for (var i = 0; i < _days.length; i++)
                  DropdownMenuItem(value: _days[i], child: Text(dayNames[i])),
                DropdownMenuItem(value: 'off', child: Text(t.cRbOff)),
              ],
              onChanged: (v) => setState(() => _day = v ?? _day),
            ),
            if (_day != 'off')
              SizedBox(
                width: 140,
                child: TextField(
                  controller: _time,
                  decoration: InputDecoration(
                    labelText: t.cRbTime,
                    isDense: true,
                    errorText: _badTime ? t.cRbInvalidTime : null,
                  ),
                ),
              ),
            OutlinedButton(onPressed: _schedule, child: Text(t.cRbApply)),
          ],
        ),
        const SizedBox(height: Space.md),
        Wrap(
          spacing: Space.sm,
          runSpacing: Space.sm,
          children: [
            OutlinedButton(
              onPressed: () => _send('backup-now', ''),
              child: Text(t.cRbBackupNow),
            ),
            OutlinedButton(
              onPressed: () => _send('update-skip', ''),
              child: Text(t.cRbSkipUpdate),
            ),
            OutlinedButton(onPressed: _restart, child: Text(t.cRbRestart)),
          ],
        ),
      ],
    );
  }
}

/// The uptime heartbeat: shows whether a ping URL is set and lets the owner set or clear it
/// (`heartbeat` job, owner code). The server agent checks the URL again before writing it.
class HeartbeatCard extends StatefulWidget {
  const HeartbeatCard({super.key, required this.configured});
  final bool configured;

  @override
  State<HeartbeatCard> createState() => _HeartbeatCardState();
}

class _HeartbeatCardState extends State<HeartbeatCard> {
  final _url = TextEditingController();
  bool _invalid = false;

  @override
  void dispose() {
    _url.dispose();
    super.dispose();
  }

  Future<void> _save(String arg) async {
    final u = Uri.tryParse(arg);
    if (arg != 'off' && (u == null || u.scheme != 'https' || u.host.isEmpty)) {
      setState(() => _invalid = true);
      return;
    }
    setState(() => _invalid = false);
    if (await sendOwnerJob(context, 'heartbeat', arg)) _url.clear();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final on = widget.configured;
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                on ? Icons.favorite_rounded : Icons.heart_broken_rounded,
                color: on ? context.colors.fresh : context.colors.sun,
              ),
              const SizedBox(width: Space.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(t.oHeartbeat, style: context.text.titleSmall),
                    Text(
                      on ? t.oHeartbeatOn : t.cRbHeartbeatOff,
                      style: context.text.bodySmall,
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: Space.sm),
          TextField(
            controller: _url,
            keyboardType: TextInputType.url,
            decoration: InputDecoration(
              labelText: t.cRbHeartbeatUrl,
              errorText: _invalid ? t.cRbHeartbeatInvalid : null,
            ),
          ),
          const SizedBox(height: Space.sm),
          Wrap(
            spacing: Space.sm,
            children: [
              OutlinedButton(
                onPressed: () => _save(_url.text.trim()),
                child: Text(t.save),
              ),
              if (on)
                TextButton(
                  onPressed: () => _save('off'),
                  child: Text(t.cRbOff),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

/// The setup key from an `otpauth://` URI, or null.
String? otpSecret(String uri) =>
    Uri.tryParse(uri.trim())?.queryParameters['secret'];

/// Sets up the server approval code once (`host-approval-enroll`, owner code). The agent makes the secret;
/// the console shows it only for this visit, with copy buttons as in the staff code setup.
class ApprovalEnrollCard extends StatefulWidget {
  const ApprovalEnrollCard({super.key});

  @override
  State<ApprovalEnrollCard> createState() => _ApprovalEnrollCardState();
}

class _ApprovalEnrollCardState extends State<ApprovalEnrollCard> {
  bool _busy = false;
  String? _uri;
  String? _error;

  Future<void> _start() async {
    final api = context.api, t = context.t;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final job = await runOwnerAction(
        context,
        (h) => queueHostJob(api, 'host-approval-enroll', '', h),
        done: t.cRbEnrollWaiting,
      );
      if (job is! Json) return;
      final end = await waitHostJob(api, job['id'] as String);
      final uri = end['state'] == 'ok'
          ? await hostJobOutput(api, job['id'] as String)
          : null;
      if (!mounted) return;
      setState(() {
        _uri = uri?.trim();
        if (uri == null) {
          _error = t.cRbEnrollFailed(jobReason(end) ?? t.cRbDomainNoReason);
        }
      });
    } catch (e) {
      if (mounted) setState(() => _error = t.error(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final uri = _uri;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(t.cRbEnrollTitle, style: context.text.labelLarge),
        const SizedBox(height: Space.xs),
        Text(t.cRbEnrollBody, style: context.text.bodySmall),
        if (uri != null) ...[
          CopyRow(label: t.cRbEnrollSecret, value: otpSecret(uri) ?? ''),
          CopyRow(label: t.cRbEnrollUri, value: uri),
          Text(t.cRbEnrollDone, style: context.text.bodySmall),
        ] else ...[
          if (_error case final e?)
            Padding(
              padding: const EdgeInsets.only(top: Space.xs),
              child: Text(
                e,
                style: context.text.bodySmall?.copyWith(
                  color: context.colors.danger,
                ),
              ),
            ),
          const SizedBox(height: Space.sm),
          OutlinedButton(
            onPressed: _busy ? null : _start,
            child: Text(_busy ? t.cRbEnrollWaiting : t.cRbEnrollStart),
          ),
        ],
      ],
    );
  }
}
