var o=Object.defineProperty;var r=(n,t,i)=>t in n?o(n,t,{enumerable:!0,configurable:!0,writable:!0,value:i}):n[t]=i;var s=(n,t,i)=>r(n,typeof t!="symbol"?t+"":t,i);class e extends String{constructor(i,a){super(i);s(this,"__apiType");s(this,"value");s(this,"__meta__");this.value=i,this.__meta__=a}toString(){return this.value}}const c=new e(`
    query Daemon {
  daemonStatus {
    healthy
    status
    statusRaw
    runnerConnected
    activeAgents
    maxAgents
    projectRoot
  }
  daemonHealth {
    healthy
    status
    runnerConnected
    runnerPid
    activeAgents
    daemonPid
  }
  agentRuns {
    runId
    taskId
    taskTitle
    workflowId
    phaseId
    status
  }
  daemonLogs(limit: 50) {
    timestamp
    level
    message
  }
}
    `),u=new e(`
    mutation DaemonStart {
  daemonStart
}
    `),p=new e(`
    mutation DaemonStop {
  daemonStop
}
    `),m=new e(`
    mutation DaemonPause {
  daemonPause
}
    `),$=new e(`
    mutation DaemonResume {
  daemonResume
}
    `),l=new e(`
    mutation DaemonClearLogs {
  daemonClearLogs
}
    `),w=new e(`
    query Dashboard {
  taskStats {
    total
    byStatus
    byPriority
  }
  daemonHealth {
    healthy
    status
    runnerConnected
    activeDaemons: activeAgents
    daemonPid
  }
  agentRuns {
    runId
    taskId
    taskTitle
    workflowId
    phaseId
    status
  }
  systemInfo {
    platform
    version
    daemonStatus
    projectRoot
  }
}
    `),k=new e(`
    query ReadyTasks($search: String, $limit: Int) {
  readyTasks(search: $search, limit: $limit) {
    id
    title
    statusRaw
    priorityRaw
    taskTypeRaw
  }
}
    `),y=new e(`
    query WorkflowDefinitions {
  workflowDefinitions {
    id
    name
    description
    phases
  }
}
    `),D=new e(`
    subscription DaemonEvents($eventType: String) {
  daemonEvents(eventType: $eventType) {
    id
    seq
    timestamp
    eventType
    data
  }
}
    `);new e(`
    query Vision {
  vision {
    title
    summary
    goals
    targetAudience
    successCriteria
    constraints
    raw
  }
}
    `);new e(`
    query Requirements {
  requirements {
    id
    title
    description
    priority
    priorityRaw
    status
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `);new e(`
    query Requirement($id: ID!) {
  requirement(id: $id) {
    id
    title
    description
    priority
    priorityRaw
    status
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `);const g=new e(`
    query RequirementDetail($id: ID!) {
  requirement(id: $id) {
    id
    title
    description
    priority
    priorityRaw
    status
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `);new e(`
    mutation SaveVision($content: String!) {
  saveVision(content: $content) {
    title
    summary
    goals
    targetAudience
    successCriteria
    constraints
    raw
  }
}
    `);new e(`
    mutation RefineVision($feedback: String) {
  refineVision(feedback: $feedback) {
    title
    summary
    goals
    targetAudience
    successCriteria
    constraints
    raw
  }
}
    `);new e(`
    mutation CreateRequirement($title: String!, $description: String, $priority: String, $requirementType: String, $acceptanceCriteria: [String!]) {
  createRequirement(
    title: $title
    description: $description
    priority: $priority
    requirementType: $requirementType
    acceptanceCriteria: $acceptanceCriteria
  ) {
    id
  }
}
    `);new e(`
    mutation UpdateRequirement($id: ID!, $title: String, $description: String, $priority: String, $status: String, $requirementType: String, $acceptanceCriteria: [String!]) {
  updateRequirement(
    id: $id
    title: $title
    description: $description
    priority: $priority
    status: $status
    requirementType: $requirementType
    acceptanceCriteria: $acceptanceCriteria
  ) {
    id
  }
}
    `);new e(`
    mutation DeleteRequirement($id: ID!) {
  deleteRequirement(id: $id)
}
    `);new e(`
    mutation DraftRequirement($context: String) {
  draftRequirement(context: $context) {
    id
    title
  }
}
    `);new e(`
    mutation RefineRequirement($id: String!, $feedback: String) {
  refineRequirement(id: $id, feedback: $feedback) {
    id
  }
}
    `);const S=new e(`
    query Projects {
  projects {
    id
    name
    path
    description
    archived
  }
  projectsActive {
    id
    name
    path
  }
}
    `),R=new e(`
    query ProjectDetail($id: ID!) {
  project(id: $id) {
    id
    name
    path
    description
    type
    techStack
    archived
  }
}
    `),I=new e(`
    mutation CreateProject($name: String!, $path: String!, $description: String, $projectType: String) {
  createProject(
    name: $name
    path: $path
    description: $description
    projectType: $projectType
  ) {
    id
    name
  }
}
    `),h=new e(`
    mutation UpdateProject($id: ID!, $name: String, $description: String, $projectType: String) {
  updateProject(
    id: $id
    name: $name
    description: $description
    projectType: $projectType
  ) {
    id
    name
  }
}
    `),T=new e(`
    mutation DeleteProject($id: ID!) {
  deleteProject(id: $id)
}
    `),f=new e(`
    mutation LoadProject($id: ID!) {
  loadProject(id: $id) {
    id
    name
  }
}
    `),q=new e(`
    mutation ArchiveProject($id: ID!) {
  archiveProject(id: $id) {
    id
    name
    archived
  }
}
    `),P=new e(`
    query Queue {
  queue {
    taskId
    title
    priority
    status
    waitTime
    position
  }
  queueStats {
    depth
    readyCount
    heldCount
    avgWait
    throughput
  }
}
    `),v=new e(`
    mutation QueueHold($taskId: String!, $reason: String) {
  queueHold(taskId: $taskId, reason: $reason)
}
    `),C=new e(`
    mutation QueueRelease($taskId: String!) {
  queueRelease(taskId: $taskId)
}
    `),j=new e(`
    mutation QueueReorder($taskIds: [String!]!) {
  queueReorder(taskIds: $taskIds)
}
    `),A=new e(`
    mutation ReviewHandoff($targetRole: String!, $question: String!, $context: String) {
  reviewHandoff(targetRole: $targetRole, question: $question, context: $context)
}
    `),W=new e(`
    query Tasks($status: String, $search: String) {
  tasks(status: $status, search: $search) {
    id
    title
    status
    statusRaw
    priority
    priorityRaw
    taskType
    taskTypeRaw
    tags
    linkedRequirementIds
  }
  taskStats {
    total
    byStatus
    byPriority
  }
}
    `);new e(`
    query TasksPrioritized {
  tasksPrioritized {
    id
    title
    status
    statusRaw
    priority
    priorityRaw
    taskType
    taskTypeRaw
    tags
  }
}
    `);const b=new e(`
    query TaskDetail($id: ID!) {
  task(id: $id) {
    id
    title
    description
    status
    statusRaw
    priority
    priorityRaw
    taskType
    taskTypeRaw
    risk
    scope
    complexity
    tags
    linkedRequirementIds
    checklist {
      id
      description
      completed
    }
    dependencies {
      taskId
      type
    }
  }
}
    `),x=new e(`
    mutation UpdateTaskStatus($id: ID!, $status: String!) {
  updateTaskStatus(id: $id, status: $status) {
    id
    status
    statusRaw
  }
}
    `),H=new e(`
    mutation CreateTask($title: String!, $description: String, $taskType: String, $priority: String) {
  createTask(
    title: $title
    description: $description
    taskType: $taskType
    priority: $priority
  ) {
    id
    title
    status
    statusRaw
  }
}
    `),U=new e(`
    mutation UpdateTask($id: ID!, $title: String, $description: String, $taskType: String, $priority: String, $risk: String, $scope: String, $complexity: String) {
  updateTask(
    id: $id
    title: $title
    description: $description
    taskType: $taskType
    priority: $priority
    risk: $risk
    scope: $scope
    complexity: $complexity
  ) {
    id
    title
    status
    statusRaw
  }
}
    `),_=new e(`
    mutation DeleteTask($id: ID!) {
  deleteTask(id: $id)
}
    `),Q=new e(`
    mutation AssignAgent($id: ID!, $role: String, $model: String) {
  assignAgent(id: $id, role: $role, model: $model) {
    id
  }
}
    `),L=new e(`
    mutation AssignHuman($id: ID!, $name: String!) {
  assignHuman(id: $id, name: $name) {
    id
  }
}
    `),O=new e(`
    mutation ChecklistAdd($id: ID!, $description: String!) {
  checklistAdd(id: $id, description: $description) {
    id
    checklist {
      id
      description
      completed
    }
  }
}
    `),V=new e(`
    mutation ChecklistUpdate($id: ID!, $itemId: String!, $completed: Boolean, $description: String) {
  checklistUpdate(
    id: $id
    itemId: $itemId
    completed: $completed
    description: $description
  ) {
    id
    checklist {
      id
      description
      completed
    }
  }
}
    `),E=new e(`
    mutation DependencyAdd($id: ID!, $dependsOn: String!, $dependencyType: String) {
  dependencyAdd(id: $id, dependsOn: $dependsOn, dependencyType: $dependencyType) {
    id
    dependencies {
      taskId
      type
    }
  }
}
    `),z=new e(`
    mutation DependencyRemove($id: ID!, $dependsOn: String!) {
  dependencyRemove(id: $id, dependsOn: $dependsOn) {
    id
    dependencies {
      taskId
      type
    }
  }
}
    `),B=new e(`
    query Workflows($status: String) {
  workflows(status: $status) {
    id
    taskId
    workflowRef
    status
    statusRaw
    currentPhase
    totalReworks
    phases {
      phaseId
      status
      startedAt
      completedAt
      attempt
      errorMessage
    }
  }
}
    `),M=new e(`
    query WorkflowDetail($id: ID!) {
  workflow(id: $id) {
    id
    taskId
    workflowRef
    status
    statusRaw
    currentPhase
    totalReworks
    phases {
      phaseId
      status
      startedAt
      completedAt
      attempt
      errorMessage
    }
    decisions {
      timestamp
      phaseId
      source
      decision
      targetPhase
      reason
      confidence
      risk
    }
  }
  workflowCheckpoints(workflowId: $id) {
    id
    phase
    timestamp
    data
  }
}
    `),F=new e(`
    mutation RunWorkflow($taskId: String!, $workflowRef: String) {
  runWorkflow(taskId: $taskId, workflowRef: $workflowRef) {
    id
    taskId
    status
    statusRaw
  }
}
    `),G=new e(`
    mutation PauseWorkflow($id: ID!) {
  pauseWorkflow(id: $id) {
    id
    status
  }
}
    `),J=new e(`
    mutation ResumeWorkflow($id: ID!) {
  resumeWorkflow(id: $id) {
    id
    status
  }
}
    `),K=new e(`
    mutation CancelWorkflow($id: ID!) {
  cancelWorkflow(id: $id) {
    id
    status
  }
}
    `),N=new e(`
    mutation ApprovePhase($workflowId: ID!, $phaseId: String!, $note: String) {
  approvePhase(workflowId: $workflowId, phaseId: $phaseId, note: $note) {
    id
    status
    statusRaw
    currentPhase
  }
}
    `);export{q as A,C as B,I as C,w as D,j as E,D as F,A as G,y as H,k as I,f as L,R as P,P as Q,g as R,b as T,h as U,M as W,c as a,u as b,p as c,m as d,$ as e,l as f,S as g,T as h,W as i,H as j,x as k,U as l,_ as m,Q as n,L as o,O as p,V as q,E as r,z as s,B as t,F as u,G as v,J as w,K as x,N as y,v as z};
