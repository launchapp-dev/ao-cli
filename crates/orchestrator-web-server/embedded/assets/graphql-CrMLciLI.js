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
    `);const y=new e(`
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
    `);const g=new e(`
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
    `),D=new e(`
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
    `),S=new e(`
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
    `),R=new e(`
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
    `),I=new e(`
    mutation DeleteProject($id: ID!) {
  deleteProject(id: $id)
}
    `),h=new e(`
    mutation LoadProject($id: ID!) {
  loadProject(id: $id) {
    id
    name
  }
}
    `),T=new e(`
    mutation ArchiveProject($id: ID!) {
  archiveProject(id: $id) {
    id
    name
    archived
  }
}
    `),f=new e(`
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
    `),q=new e(`
    mutation QueueHold($taskId: String!, $reason: String) {
  queueHold(taskId: $taskId, reason: $reason)
}
    `),P=new e(`
    mutation QueueRelease($taskId: String!) {
  queueRelease(taskId: $taskId)
}
    `),v=new e(`
    mutation QueueReorder($taskIds: [String!]!) {
  queueReorder(taskIds: $taskIds)
}
    `),C=new e(`
    mutation ReviewHandoff($targetRole: String!, $question: String!, $context: String) {
  reviewHandoff(targetRole: $targetRole, question: $question, context: $context)
}
    `),j=new e(`
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
    `);const A=new e(`
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
    `),W=new e(`
    mutation UpdateTaskStatus($id: ID!, $status: String!) {
  updateTaskStatus(id: $id, status: $status) {
    id
    status
    statusRaw
  }
}
    `),b=new e(`
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
    `),x=new e(`
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
    `),H=new e(`
    mutation DeleteTask($id: ID!) {
  deleteTask(id: $id)
}
    `),U=new e(`
    mutation AssignAgent($id: ID!, $role: String, $model: String) {
  assignAgent(id: $id, role: $role, model: $model) {
    id
  }
}
    `),_=new e(`
    mutation AssignHuman($id: ID!, $name: String!) {
  assignHuman(id: $id, name: $name) {
    id
  }
}
    `),Q=new e(`
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
    `),L=new e(`
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
    `),O=new e(`
    mutation DependencyAdd($id: ID!, $dependsOn: String!, $dependencyType: String) {
  dependencyAdd(id: $id, dependsOn: $dependsOn, dependencyType: $dependencyType) {
    id
    dependencies {
      taskId
      type
    }
  }
}
    `),V=new e(`
    mutation DependencyRemove($id: ID!, $dependsOn: String!) {
  dependencyRemove(id: $id, dependsOn: $dependsOn) {
    id
    dependencies {
      taskId
      type
    }
  }
}
    `),E=new e(`
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
    `),z=new e(`
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
    `),B=new e(`
    mutation RunWorkflow($taskId: String!, $workflowRef: String) {
  runWorkflow(taskId: $taskId, workflowRef: $workflowRef) {
    id
    taskId
    status
    statusRaw
  }
}
    `),M=new e(`
    mutation PauseWorkflow($id: ID!) {
  pauseWorkflow(id: $id) {
    id
    status
  }
}
    `),F=new e(`
    mutation ResumeWorkflow($id: ID!) {
  resumeWorkflow(id: $id) {
    id
    status
  }
}
    `),G=new e(`
    mutation CancelWorkflow($id: ID!) {
  cancelWorkflow(id: $id) {
    id
    status
  }
}
    `),J=new e(`
    mutation ApprovePhase($workflowId: ID!, $phaseId: String!, $note: String) {
  approvePhase(workflowId: $workflowId, phaseId: $phaseId, note: $note) {
    id
    status
    statusRaw
    currentPhase
  }
}
    `);export{T as A,v as B,S as C,w as D,k as E,C as F,B as G,h as L,D as P,f as Q,y as R,A as T,R as U,z as W,c as a,u as b,p as c,m as d,$ as e,l as f,g,I as h,j as i,b as j,W as k,x as l,H as m,U as n,_ as o,Q as p,L as q,O as r,V as s,E as t,M as u,G as v,J as w,F as x,q as y,P as z};
