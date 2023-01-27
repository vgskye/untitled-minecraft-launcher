<script lang="ts">
  import Greet from "$lib/Greet.svelte";
  import { invoke } from "@tauri-apps/api/tauri";
  import { listen } from '@tauri-apps/api/event';

  import { info } from "tauri-plugin-log-api";

  let msg: string;
  async function msa_login() { 
    info(await invoke("login_msa"))
  }
  listen<string>("auth:msa:login_message", (e) => {
    msg = e.payload
  })
</script>

<nav class="navbar bg-base-200 w-screen fixed top-0 min-h-8">
  <a class="btn btn-ghost btn-sm" href="/new">
    New Instance
  </a>
  <button on:click={msa_login}>
  Log in
  </button>
</nav>

<div class="mt-12">
  {msg}
</div>

<style lang="postcss">

</style>