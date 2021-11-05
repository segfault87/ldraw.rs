// Portions of shader code has been taken from Three.js source code.
//
// Copyright © 2010-2021 three.js authors

precision mediump float;

in vec3 position;
in vec3 control1;
in vec3 control2;
in vec3 direction;
in vec3 color;

#ifdef USE_INSTANCING
    in vec4 instancedColor;
    in vec4 instancedEdgeColor;
    in mat4 instancedModelMatrix;
#else
    uniform vec4 defaultColor;
    uniform vec4 edgeColor;
#endif

uniform mat4 modelMatrix;
uniform mat4 projection;
uniform mat4 modelView;

out vec4 vColor;
out float discardFlag;

void main(void) {
    mat4 mvp = projection * modelView;

    #ifdef USE_INSTANCING
        mvp = mvp * instancedModelMatrix;
    #endif

    vec4 c1 = mvp * vec4( control1, 1.0 );
    vec4 c2 = mvp * vec4( control2, 1.0 );
    vec4 p1 = mvp * vec4( position, 1.0 );
    vec4 p2 = mvp * vec4( position + direction, 1.0 );

    c1.xy /= c1.w;
    c2.xy /= c2.w;
    p1.xy /= p1.w;
    p2.xy /= p2.w;
    // Get the direction of the segment and an orthogonal vector
    vec2 dir = p2.xy - p1.xy;
    vec2 norm = vec2( -dir.y, dir.x );
    // Get control point directions from the line
    vec2 c1dir = c1.xy - p2.xy;
    vec2 c2dir = c2.xy - p2.xy;
    // If the vectors to the controls points are pointed in different directions away
    // from the line segment then the line should not be drawn.
    float d0 = dot( normalize( norm ), normalize( c1dir ) );
    float d1 = dot( normalize( norm ), normalize( c2dir ) );
    discardFlag = float( sign( d0 ) != sign( d1 ) );

    vec4 mvPosition = vec4(position, 1.0);
    #ifdef USE_INSTANCING
        mvPosition = instancedModelMatrix * mvPosition;
    #endif
    mvPosition = modelView * mvPosition;
    gl_Position = projection * mvPosition;
    #ifdef USE_INSTANCING
        if (color.x < -1.0) {
            vColor = instancedEdgeColor;
        } else if (color.x < 0.0) {
            vColor = instancedColor;
        } else {
            vColor = vec4(color, 1.0);
        }
    #else
        if (color.x < -1.0) {
            vColor = edgeColor;
        } else if (color.x < 0.0) {
            vColor = defaultColor;
        } else {
            vColor = vec4(color, 1.0);
        }
    #endif
}
