var o=Object.defineProperty;var r=(t,i,n)=>i in t?o(t,i,{enumerable:!0,configurable:!0,writable:!0,value:n}):t[i]=n;var s=(t,i,n)=>r(t,typeof i!="symbol"?i+"":i,n);var d=(t=>(t.Cancelled="CANCELLED",t.Completed="COMPLETED",t.Escalated="ESCALATED",t.Failed="FAILED",t.Paused="PAUSED",t.Pending="PENDING",t.Running="RUNNING",t))(d||{});class e extends String{constructor(n,a){super(n);s(this,"__apiType");s(this,"value");s(this,"__meta__");this.value=n,this.__meta__=a}toString(){return this.value}}new e(`
    mutation UpsertWorkflowDefinition($id: String!, $name: String!, $description: String, $phases: String!, $variables: String) {
  upsertWorkflowDefinition(
    id: $id
    name: $name
    description: $description
    phases: $phases
    variables: $variables
  )
}
    `);new e(`
    mutation DeleteWorkflowDefinition($id: ID!) {
  deleteWorkflowDefinition(id: $id)
}
    `);const u=new e(`
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
    `),p=new e(`
    mutation DaemonStart {
  daemonStart
}
    `),m=new e(`
    mutation DaemonStop {
  daemonStop
}
    `),$=new e(`
    mutation DaemonPause {
  daemonPause
}
    `),l=new e(`
    mutation DaemonResume {
  daemonResume
}
    `),w=new e(`
    mutation DaemonClearLogs {
  daemonClearLogs
}
    `),k=new e(`
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
    `),D=new e(`
    query ReadyTasks($search: String, $limit: Int) {
  readyTasks(search: $search, limit: $limit) {
    id
    title
    statusRaw
    priorityRaw
    taskTypeRaw
  }
}
    `),g=new e(`
    query WorkflowDefinitions {
  workflowDefinitions {
    id
    name
    description
    phases
  }
}
    `),y=new e(`
    query DispatchRequirements {
  requirements {
    id
    title
    description
    priorityRaw
    statusRaw
    requirementType
    tags
    linkedTaskIds
    acceptanceCriteria
  }
}
    `),R=new e(`
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
    `);const S=new e(`
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
    `);const f=new e(`
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
    `),I=new e(`
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
    `),h=new e(`
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
    `),T=new e(`
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
    `),q=new e(`
    mutation DeleteProject($id: ID!) {
  deleteProject(id: $id)
}
    `),P=new e(`
    mutation LoadProject($id: ID!) {
  loadProject(id: $id) {
    id
    name
  }
}
    `),v=new e(`
    mutation ArchiveProject($id: ID!) {
  archiveProject(id: $id) {
    id
    name
    archived
  }
}
    `),C=new e(`
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
    `),A=new e(`
    mutation QueueHold($taskId: String!, $reason: String) {
  queueHold(taskId: $taskId, reason: $reason)
}
    `),j=new e(`
    mutation QueueRelease($taskId: String!) {
  queueRelease(taskId: $taskId)
}
    `),b=new e(`
    mutation QueueReorder($taskIds: [String!]!) {
  queueReorder(taskIds: $taskIds)
}
    `),W=new e(`
    mutation ReviewHandoff($targetRole: String!, $question: String!, $context: String) {
  reviewHandoff(targetRole: $targetRole, question: $question, context: $context)
}
    `),x=new e(`
    query WorkflowConfig {
  workflowConfig {
    mcpServers {
      name
      command
      args
      transport
      tools
      env {
        key
        value
      }
    }
    phaseCatalog {
      id
      label
      description
      category
      tags
    }
    tools {
      name
      executable
      supportsMcp
      supportsWrite
      contextWindow
    }
    agentProfiles {
      name
      description
      role
      mcpServers
      skills
      tool
      model
    }
    schedules {
      id
      cron
      workflowRef
      command
      enabled
    }
  }
}
    `),E=new e(`
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
    `);const U=new e(`
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
    `),H=new e(`
    mutation UpdateTaskStatus($id: ID!, $status: String!) {
  updateTaskStatus(id: $id, status: $status) {
    id
    status
    statusRaw
  }
}
    `),L=new e(`
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
    `),_=new e(`
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
    `),Q=new e(`
    mutation DeleteTask($id: ID!) {
  deleteTask(id: $id)
}
    `),O=new e(`
    mutation AssignAgent($id: ID!, $role: String, $model: String) {
  assignAgent(id: $id, role: $role, model: $model) {
    id
  }
}
    `),N=new e(`
    mutation AssignHuman($id: ID!, $name: String!) {
  assignHuman(id: $id, name: $name) {
    id
  }
}
    `),M=new e(`
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
    `),z=new e(`
    mutation DependencyAdd($id: ID!, $dependsOn: String!, $dependencyType: String) {
  dependencyAdd(id: $id, dependsOn: $dependsOn, dependencyType: $dependencyType) {
    id
    dependencies {
      taskId
      type
    }
  }
}
    `),F=new e(`
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
    `),J=new e(`
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
    `),K=new e(`
    mutation RunWorkflow($taskId: String!, $workflowRef: String) {
  runWorkflow(taskId: $taskId, workflowRef: $workflowRef) {
    id
    taskId
    status
    statusRaw
  }
}
    `),X=new e(`
    mutation PauseWorkflow($id: ID!) {
  pauseWorkflow(id: $id) {
    id
    status
  }
}
    `),Y=new e(`
    mutation ResumeWorkflow($id: ID!, $feedback: String) {
  resumeWorkflow(id: $id, feedback: $feedback) {
    id
    status
  }
}
    `),Z=new e(`
    mutation CancelWorkflow($id: ID!) {
  cancelWorkflow(id: $id) {
    id
    status
  }
}
    `),G=new e(`
    mutation ApprovePhase($workflowId: ID!, $phaseId: String!, $note: String) {
  approvePhase(workflowId: $workflowId, phaseId: $phaseId, note: $note) {
    id
    status
    statusRaw
    currentPhase
  }
}
    `);export{v as A,j as B,h as C,k as D,b as E,R as F,W as G,g as H,x as I,d as J,y as K,P as L,D as M,I as P,C as Q,S as R,U as T,T as U,J as W,u as a,p as b,m as c,$ as d,l as e,w as f,f as g,q as h,E as i,H as j,K as k,L as l,_ as m,Q as n,O as o,N as p,M as q,V as r,z as s,F as t,B as u,X as v,Y as w,Z as x,G as y,A as z};
