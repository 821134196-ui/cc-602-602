<script>
  import { onMount } from 'svelte'
  import { api } from './api.js'
  import { fmtTime } from './text.js'

  let items = []
  let open = false

  onMount(async () => {
    try { items = await api('/api/notifications') } catch {}
  })
</script>

<div class="card">
  <h2 style="cursor:pointer" on:click={() => (open = !open)}>
    通知（模拟邮件）{open ? '▾' : '▸'}
    {#if items.length > 0}<span class="badge">{items.length}</span>{/if}
  </h2>
  {#if open}
    {#each items as n}
      <div class="notif">
        <strong>{n.subject}</strong>
        <span class="muted">｜ {fmtTime(n.created_at)}</span>
        <div>{n.body}</div>
      </div>
    {/each}
    {#if items.length === 0}
      <div class="muted">暂无通知。</div>
    {/if}
  {/if}
</div>
