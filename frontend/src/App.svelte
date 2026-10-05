<script>
  import { onMount } from 'svelte'
  import { api, setToken, getToken } from './api.js'
  import Login from './Login.svelte'
  import Student from './Student.svelte'
  import Advisor from './Advisor.svelte'
  import Secretary from './Secretary.svelte'
  import Reviewer from './Reviewer.svelte'
  import Notifications from './Notifications.svelte'

  let user = null
  let ready = false

  onMount(async () => {
    if (getToken()) {
      try {
        user = await api('/api/me')
      } catch {
        setToken('')
      }
    }
    ready = true
  })

  function onLogin(e) {
    user = e.detail
  }

  async function logout() {
    try { await api('/api/logout', { method: 'POST' }) } catch {}
    setToken('')
    user = null
  }

  const ROLE_NAME = {
    student: '学生',
    advisor: '导师',
    secretary: '秘书',
    reviewer: '评阅人'
  }
</script>

{#if ready}
  {#if !user}
    <Login on:login={onLogin} />
  {:else}
    <header class="topbar">
      <h1>高校论文匿名送审与意见归档系统</h1>
      <div class="who">
        <span>{user.display_name}（{ROLE_NAME[user.role]}）</span>
        <button class="ghost" on:click={logout}>退出登录</button>
      </div>
    </header>
    <main>
      {#if user.role === 'student'}
        <Student />
      {:else if user.role === 'advisor'}
        <Advisor />
      {:else if user.role === 'secretary'}
        <Secretary />
      {:else if user.role === 'reviewer'}
        <Reviewer />
      {/if}
      <Notifications />
    </main>
  {/if}
{/if}
