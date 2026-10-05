<script>
  import { onMount } from 'svelte'
  import { api } from './api.js'
  import Login from './Login.svelte'
  import StudentView from './StudentView.svelte'
  import AdvisorView from './AdvisorView.svelte'
  import SecretaryView from './SecretaryView.svelte'
  import ReviewerView from './ReviewerView.svelte'

  let user = $state(null)
  let loading = $state(true)

  onMount(async () => {
    try { user = await api('/api/me') } catch { /* 未登录 */ }
    loading = false
  })

  async function logout() {
    await api('/api/logout', { method: 'POST', body: {} })
    user = null
  }

  const ROLE_NAMES = { student: '学生', advisor: '导师', secretary: '秘书', reviewer: '评阅人' }
</script>

<header>
  <h1>高校论文匿名送审与意见归档系统</h1>
  {#if user}
    <div class="who">
      <span>{user.display_name}（{ROLE_NAMES[user.role]}）</span>
      <button onclick={logout}>退出登录</button>
    </div>
  {/if}
</header>

<main>
  {#if loading}
    <p>加载中…</p>
  {:else if !user}
    <Login onlogin={(u) => (user = u)} />
  {:else if user.role === 'student'}
    <StudentView />
  {:else if user.role === 'advisor'}
    <AdvisorView />
  {:else if user.role === 'secretary'}
    <SecretaryView />
  {:else if user.role === 'reviewer'}
    <ReviewerView />
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: 'PingFang SC', 'Microsoft YaHei', system-ui, sans-serif;
    background: #f4f6f9;
    color: #222;
  }
  :global(button) {
    cursor: pointer;
    border: 1px solid #3468c0;
    background: #3468c0;
    color: #fff;
    border-radius: 6px;
    padding: 6px 14px;
    font-size: 14px;
  }
  :global(button:disabled) { opacity: 0.5; cursor: not-allowed; }
  :global(button.ghost) { background: #fff; color: #3468c0; }
  :global(button.danger) { background: #c0392b; border-color: #c0392b; }
  :global(input), :global(select), :global(textarea) {
    padding: 6px 8px; border: 1px solid #ccc; border-radius: 6px; font-size: 14px;
  }
  :global(.card) {
    background: #fff; border-radius: 10px; padding: 16px 20px;
    margin-bottom: 16px; box-shadow: 0 1px 3px rgba(0,0,0,0.08);
  }
  :global(.badge) {
    display: inline-block; padding: 2px 10px; border-radius: 999px;
    font-size: 12px; background: #e8eefb; color: #24509e; margin-left: 8px;
  }
  :global(.badge.frozen) { background: #e6f7ee; color: #1a7f4b; }
  :global(.muted) { color: #777; font-size: 13px; }
  :global(.error) { color: #c0392b; margin: 8px 0; }
  :global(.ok) { color: #1a7f4b; margin: 8px 0; }
  :global(table) { border-collapse: collapse; width: 100%; }
  :global(th), :global(td) { text-align: left; padding: 8px; border-bottom: 1px solid #eee; font-size: 14px; }
  header {
    background: #24509e; color: #fff; padding: 14px 28px;
    display: flex; justify-content: space-between; align-items: center;
  }
  header h1 { font-size: 18px; margin: 0; }
  header .who { display: flex; gap: 12px; align-items: center; }
  main { max-width: 1000px; margin: 24px auto; padding: 0 16px; }
</style>
